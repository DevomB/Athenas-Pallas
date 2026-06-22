//! Synchronous backtest driver.

use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::bar::{default_tick_size, BarSeries, BarSeriesSource};
use super::config::{
    instrument_meta_from_config, instrument_meta_from_extra, BacktestConfig, DataFormat,
};
use super::interval::periods_per_year_from_interval_for_class;
use super::lifecycle::apply_bar_lifecycle;
use super::merge::merge_sources_iter;
use super::pbar::is_pbar_path;
use super::sources::{FutureCsvSource, FxCsvSource, YahooCsvSource};
use super::{CsvBarSource, HistoricalSource};
use crate::calendar::{is_session_open, SessionFilter};
use crate::events::{Event, FillRecord, MarketEvent, OrderIntent};
use crate::execution::{PaperConfig, SyncPaperGateway};
use crate::metrics::{summarize_with_fills_and_rf, PerformanceSummary, RollingMetrics};
use crate::risk::{BacktestChecks, MaxDailyLossQuote, MaxPositionSize};
use crate::state::{GlobalState, InstrumentRegistry};
use crate::strategy::{Strategy, StrategyContext};
use crate::types::{Asset, EquityPoint, ExchangeId, InstrumentId, OrderType, Side, Symbol};
use crate::{dispatch_replay_bar_sync, dispatch_replay_sync};

/// JSON-serializable run output.
#[derive(Clone, Debug, Serialize)]
pub struct BacktestReport {
    /// Net PnL in quote currency.
    pub pnl: String,
    /// PnL as fraction of starting equity.
    pub pnl_pct: String,
    /// Peak-to-trough drawdown (0..1).
    pub max_drawdown: f64,
    /// Annualized Sharpe ratio.
    pub sharpe: f64,
    /// Annualized Sortino ratio.
    pub sortino: f64,
    /// Number of fills.
    pub fill_count: u64,
    /// Mark-to-market equity samples.
    pub equity_curve: Vec<EquityPoint>,
    /// Per-fill blotter when recorded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<FillRecord>,
    /// Wall-clock runtime in milliseconds.
    pub wall_time_ms: u64,
    /// Fraction of closed round-trips with positive PnL.
    pub win_rate: f64,
    /// Gross profit / gross loss.
    pub profit_factor: f64,
    /// Closed round-trip count from fill ledger.
    pub closed_trades: usize,
    /// Per-sub-strategy realized PnL when fills carry a `strategy_id` (empty otherwise).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub per_strategy: Vec<crate::metrics::StrategyPnlRow>,
}

/// Built-in buy-on-first-bar strategy when no external script is given.
pub struct BuyAndHold {
    instrument: InstrumentId,
    done: bool,
    qty: Decimal,
}

impl BuyAndHold {
    /// Buy `qty` base on the first bar with a price.
    pub fn new(instrument: InstrumentId, qty: Decimal) -> Self {
        Self {
            instrument,
            done: false,
            qty,
        }
    }
}

impl BuyAndHold {
    fn maybe_buy(&mut self, ctx: &StrategyContext, out: &mut Vec<OrderIntent>) {
        if self.done || ctx.state.mid_or_last(&self.instrument).is_none() {
            return;
        }
        self.done = true;
        out.push(OrderIntent {
            instrument: self.instrument.clone(),
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            qty: self.qty,
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        });
    }
}

impl Strategy for BuyAndHold {
    fn on_event(&mut self, ctx: &StrategyContext, _event: &Event, out: &mut Vec<OrderIntent>) {
        self.maybe_buy(ctx, out);
    }

    fn on_replay_event(
        &mut self,
        ctx: &StrategyContext<'_>,
        _event: &crate::events::ReplayEvent<'_>,
        out: &mut Vec<OrderIntent>,
    ) {
        self.maybe_buy(ctx, out);
    }

    fn uses_tick_replay(&self) -> bool {
        true
    }
}

/// Orchestrates one replay run.
pub struct BacktestRunner;

impl BacktestRunner {
    /// Run with the built-in buy-and-hold strategy.
    pub fn run_buy_and_hold(cfg: &BacktestConfig) -> crate::Result<BacktestReport> {
        Self::run_buy_and_hold_with_cancel(cfg, None)
    }

    /// Buy-and-hold with optional cooperative cancel (checked every 1024 bars).
    pub fn run_buy_and_hold_with_cancel(
        cfg: &BacktestConfig,
        cancel: Option<Arc<AtomicBool>>,
    ) -> crate::Result<BacktestReport> {
        let qty = Decimal::new(1, 2);
        let mut strategy = BuyAndHold::new(cfg.instrument.clone(), qty);
        Self::run_with_strategy_with_cancel(cfg, &mut strategy, cancel)
    }

    /// Run with any in-process or external strategy.
    pub fn run_with_strategy<S: Strategy>(
        cfg: &BacktestConfig,
        strategy: &mut S,
    ) -> crate::Result<BacktestReport> {
        Self::run_with_strategy_with_cancel(cfg, strategy, None)
    }

    /// Run with optional cooperative cancel (checked every 1024 bars).
    pub fn run_with_strategy_with_cancel<S: Strategy>(
        cfg: &BacktestConfig,
        strategy: &mut S,
        cancel: Option<Arc<AtomicBool>>,
    ) -> crate::Result<BacktestReport> {
        let started = Instant::now();
        let session = cfg
            .session_filter
            .as_deref()
            .map(SessionFilter::parse)
            .unwrap_or_default();
        let meta = instrument_meta_from_config(cfg);
        let mut instruments = HashMap::new();
        instruments.insert(cfg.instrument.clone(), meta.clone());
        for extra in &cfg.extra_instruments {
            instruments.insert(extra.instrument.clone(), instrument_meta_from_extra(extra));
        }
        let mut balances = cfg.balances.clone();
        if balances.is_empty() {
            balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
        }

        let registry = InstrumentRegistry::from_instruments(instruments);
        let mut state = GlobalState::new(registry, balances);
        state.synthetic_half_spread_bps = cfg.half_spread_bps;
        let mut checks = BacktestChecks::default();
        if let Some(max_abs) = cfg.max_position_abs {
            checks = checks.with_max_position(MaxPositionSize {
                instrument: cfg.instrument.clone(),
                max_abs,
            });
        }
        if let Some(max_loss) = cfg.max_daily_loss_quote {
            let quote = meta.quote.clone();
            state.daily_risk_quote = Some(quote.clone());
            checks = checks.with_max_daily_loss(MaxDailyLossQuote { quote, max_loss });
        }
        let paper = PaperConfig {
            fee_bps: cfg.fee_bps,
            market_slippage_bps: cfg.slippage_bps,
            ..PaperConfig::default()
        };
        let exec = SyncPaperGateway::new(paper);

        let exchange = ExchangeId::new(cfg.instrument.exchange.as_str());
        let symbol = Symbol::new(cfg.instrument.symbol.as_str());
        let fmt = match cfg.data_format {
            DataFormat::Auto => detect_format(&cfg.data_path)?,
            other => other,
        };
        let mut ohlcv_series = if matches!(fmt, DataFormat::Ohlcv) || is_pbar_path(&cfg.data_path) {
            BarSeries::from_csv_path_or_pbar(&cfg.data_path, default_tick_size()).ok()
        } else {
            None
        };
        let multi_instrument = cfg.extra_instruments.iter().any(|e| e.data_path.is_some());
        let periods_per_year = resolve_periods_per_year(cfg, ohlcv_series.as_ref());
        let bar_count = ohlcv_series.as_ref().map_or(0, BarSeries::len);
        let mut equity = if cfg.record_equity_curve {
            Vec::with_capacity(bar_count.max(1))
        } else {
            Vec::new()
        };
        let mut metrics = RollingMetrics::new();
        let mut intents = Vec::with_capacity(4);
        let inst_ix = 0usize;

        if strategy.uses_tick_replay() && matches!(fmt, DataFormat::Ohlcv) && !multi_instrument {
            if let Some(series) = ohlcv_series.take() {
                let tick = series.tick_size();
                let mut src = BarSeriesSource::new(series, exchange.clone(), symbol.clone());
                let mut bar_ix = 0u64;
                while let Some((bar, ts)) = src.next_bar() {
                    if !is_session_open(session, ts) {
                        continue;
                    }
                    bar_ix += 1;
                    if bar_ix % 64 == 0 {
                        if let Some(ref hook) = cfg.on_progress {
                            hook(&format!("processed {bar_ix} bars"));
                        }
                        if cancelled(&cancel) {
                            return Err(crate::Error::Cancelled);
                        }
                    }
                    state.apply_bar(inst_ix, &bar, tick, cfg.half_spread_bps);
                    state.refresh_daily_risk_anchor(ts);
                    let rev = src.bar_to_replay_event(&bar, ts);
                    dispatch_replay_bar_sync(
                        &mut state,
                        strategy,
                        &checks,
                        &exec,
                        &rev,
                        &mut intents,
                    )?;
                    apply_bar_lifecycle(&mut state, ts);
                    if let Some(eq) = record_equity(&state, multi_instrument, inst_ix) {
                        metrics.record(eq, periods_per_year);
                        if cfg.record_equity_curve {
                            equity.push(EquityPoint {
                                ts,
                                equity_quote: eq,
                            });
                        }
                    }
                }
                let fills = state.take_fill_log();
                let report = finalize_report(
                    cfg,
                    equity,
                    &metrics,
                    periods_per_year,
                    &fills,
                    state.fill_count,
                    started,
                );
                return Ok(report);
            }
        }

        if multi_instrument {
            let mut sources = load_all_sources(cfg, exchange, symbol, fmt, ohlcv_series)?;
            let mut bar_ix = 0u64;
            for ev in merge_sources_iter(&mut sources) {
                let ts = event_ts(&ev);
                if !is_session_open(session, ts) {
                    continue;
                }
                bar_ix += 1;
                if bar_ix % 64 == 0 {
                    if let Some(ref hook) = cfg.on_progress {
                        hook(&format!("processed {bar_ix} bars"));
                    }
                    if cancelled(&cancel) {
                        return Err(crate::Error::Cancelled);
                    }
                }
                if let Event::Market(ref m) = ev {
                    state.apply_market(m);
                    state.refresh_daily_risk_anchor(ts);
                    if matches!(m, MarketEvent::Bar { .. }) {
                        apply_bar_lifecycle(&mut state, ts);
                    }
                }
                dispatch_replay_sync(&mut state, strategy, &checks, &exec, ev, &mut intents)?;
                if let Some(eq) = record_equity(&state, true, inst_ix) {
                    metrics.record(eq, periods_per_year);
                    if cfg.record_equity_curve {
                        equity.push(EquityPoint {
                            ts,
                            equity_quote: eq,
                        });
                    }
                }
            }
        } else {
            let mut src = load_source(cfg, exchange, symbol, fmt, ohlcv_series)?;
            let mut bar_ix = 0u64;
            while let Some(ev) = src.next_event() {
                let ts = event_ts(&ev);
                if !is_session_open(session, ts) {
                    continue;
                }
                bar_ix += 1;
                if bar_ix % 64 == 0 {
                    if let Some(ref hook) = cfg.on_progress {
                        hook(&format!("processed {bar_ix} bars"));
                    }
                    if cancelled(&cancel) {
                        return Err(crate::Error::Cancelled);
                    }
                }
                if let Event::Market(ref m) = ev {
                    state.apply_market(m);
                    state.refresh_daily_risk_anchor(ts);
                    if matches!(m, MarketEvent::Bar { .. }) {
                        apply_bar_lifecycle(&mut state, ts);
                    }
                }
                dispatch_replay_sync(&mut state, strategy, &checks, &exec, ev, &mut intents)?;
                if let Some(eq) = record_equity(&state, false, inst_ix) {
                    metrics.record(eq, periods_per_year);
                    if cfg.record_equity_curve {
                        equity.push(EquityPoint {
                            ts,
                            equity_quote: eq,
                        });
                    }
                }
            }
        }

        let fills = state.take_fill_log();
        let report = finalize_report(
            cfg,
            equity,
            &metrics,
            periods_per_year,
            &fills,
            state.fill_count,
            started,
        );
        Ok(report)
    }
}

/// Build the report from either the recorded equity curve or streamed [`RollingMetrics`].
///
/// When `record_equity_curve` is false, `equity` is empty and metrics come from the O(1)
/// streaming accumulator instead of a materialized `Vec<EquityPoint>`.
fn finalize_report(
    cfg: &BacktestConfig,
    equity: Vec<EquityPoint>,
    metrics: &RollingMetrics,
    periods_per_year: f64,
    fills: &[FillRecord],
    fill_count: u64,
    started: Instant,
) -> BacktestReport {
    let summary = if cfg.record_equity_curve {
        summarize_with_fills_and_rf(equity, periods_per_year, fills, cfg.risk_free_annual)
    } else {
        metrics.streaming_summary(periods_per_year, fills, cfg.risk_free_annual)
    };
    let mut report = report_from_summary(
        summary,
        fill_count,
        started.elapsed().as_millis() as u64,
        fills.to_vec(),
    );
    if report.max_drawdown == 0.0 {
        report.max_drawdown = metrics.max_drawdown();
    }
    report
}

fn cancelled(cancel: &Option<Arc<AtomicBool>>) -> bool {
    cancel.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
}

fn resolve_periods_per_year(cfg: &BacktestConfig, series: Option<&BarSeries>) -> f64 {
    if !cfg.auto_periods_per_year {
        return cfg.periods_per_year;
    }
    if let Some(iv) = &cfg.bar_interval {
        return periods_per_year_from_interval_for_class(iv, cfg.asset_class);
    }
    if let Some(series) = series {
        if series.len() >= 2 {
            return series.infer_periods_per_year(cfg.asset_class);
        }
    }
    cfg.periods_per_year
}

fn record_equity(state: &GlobalState, multi: bool, primary_ix: usize) -> Option<Decimal> {
    if multi {
        Some(state.portfolio_equity())
    } else {
        state.mark_to_market_equity_ix(primary_ix)
    }
}

fn load_all_sources(
    cfg: &BacktestConfig,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
    ohlcv_series: Option<BarSeries>,
) -> crate::Result<Vec<Box<dyn HistoricalSource>>> {
    let mut out = vec![load_source(cfg, exchange, symbol, fmt, ohlcv_series)?];
    for extra in &cfg.extra_instruments {
        let Some(path) = &extra.data_path else {
            continue;
        };
        let ex = ExchangeId::new(extra.instrument.exchange.as_str());
        let sym = Symbol::new(extra.instrument.symbol.as_str());
        let fmt = extra.data_format.unwrap_or(DataFormat::Auto);
        let fmt = if fmt == DataFormat::Auto {
            detect_format(path)?
        } else {
            fmt
        };
        out.push(open_source_path(path, ex, sym, fmt)?);
    }
    Ok(out)
}

fn open_source_path(
    path: &Path,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
) -> crate::Result<Box<dyn HistoricalSource>> {
    Ok(match fmt {
        DataFormat::Ohlcv => Box::new(CsvBarSource::from_path(path, exchange, symbol)?),
        DataFormat::Yahoo => Box::new(YahooCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Fx => Box::new(FxCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Future => Box::new(FutureCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Auto => unreachable!(),
    })
}

fn load_source(
    cfg: &BacktestConfig,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
    ohlcv_series: Option<BarSeries>,
) -> crate::Result<Box<dyn HistoricalSource>> {
    let path = &cfg.data_path;
    Ok(match fmt {
        DataFormat::Ohlcv => {
            if let Some(series) = ohlcv_series {
                Box::new(BarSeriesSource::new(series, exchange, symbol))
            } else if is_pbar_path(path) {
                let series = BarSeries::from_csv_path_or_pbar(path, default_tick_size())
                    .map_err(crate::error::Error::Io)?;
                Box::new(BarSeriesSource::new(series, exchange, symbol))
            } else {
                Box::new(CsvBarSource::from_path(path, exchange, symbol)?)
            }
        }
        DataFormat::Yahoo => Box::new(YahooCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Fx => Box::new(FxCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Future => Box::new(FutureCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Auto => unreachable!(),
    })
}

fn detect_format(path: &Path) -> crate::Result<DataFormat> {
    let mut rdr = csv::Reader::from_path(path).map_err(|e| {
        crate::error::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;
    let headers = rdr
        .headers()
        .map_err(|e| {
            crate::error::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?
        .clone();
    if headers.iter().any(|h| h == "Date") {
        return Ok(DataFormat::Yahoo);
    }
    if headers.iter().any(|h| h == "bid") {
        return Ok(DataFormat::Fx);
    }
    Ok(DataFormat::Ohlcv)
}

fn event_ts(ev: &Event) -> time::OffsetDateTime {
    ev.timestamp_or_now()
}

fn report_from_summary(
    s: PerformanceSummary,
    fill_count: u64,
    wall_time_ms: u64,
    fills: Vec<FillRecord>,
) -> BacktestReport {
    let per_strategy = crate::metrics::per_strategy_pnl(&fills);
    BacktestReport {
        pnl: s.pnl.to_string(),
        pnl_pct: s.pnl_pct.to_string(),
        max_drawdown: s.max_drawdown,
        sharpe: s.sharpe,
        sortino: s.sortino,
        fill_count,
        equity_curve: s.equity,
        fills,
        wall_time_ms,
        win_rate: s.win_rate,
        profit_factor: s.profit_factor,
        closed_trades: s.closed_trades,
        per_strategy,
    }
}

impl BacktestReport {
    /// Write pretty JSON to disk.
    pub fn write_json(&self, path: &Path) -> crate::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let mut f = File::create(path)?;
        f.write_all(json.as_bytes())?;
        Ok(())
    }
}
