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
use super::config::{parse_base_quote, BacktestConfig, DataFormat};
use super::sources::{FutureCsvSource, FxCsvSource, YahooCsvSource};
use super::{CsvBarSource, HistoricalSource};
use crate::dispatch_replay_sync;
use crate::events::FillRecord;
use crate::events::{Event, OrderIntent};
use crate::execution::{PaperConfig, SyncPaperGateway};
use crate::instrument::{AssetClass, InstrumentMeta};
use crate::metrics::{summarize, PerformanceSummary, RollingMetrics};
use crate::risk::BacktestChecks;
use crate::state::{GlobalState, InstrumentRegistry};
use crate::strategy::{Strategy, StrategyContext};
use crate::types::{Asset, EquityPoint, ExchangeId, InstrumentId, OrderType, Side, Symbol};

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

impl Strategy for BuyAndHold {
    fn on_event(&mut self, ctx: &StrategyContext, _event: &Event, out: &mut Vec<OrderIntent>) {
        if self.done || ctx.state.mid_or_last(&self.instrument).is_none() {
            return;
        }
        self.done = true;
        out.push(OrderIntent {
            instrument: self.instrument.clone(),
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            qty: self.qty,
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        });
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
        let meta = instrument_meta_from_config(cfg);
        let mut instruments = HashMap::new();
        instruments.insert(cfg.instrument.clone(), meta.clone());
        let mut balances = cfg.balances.clone();
        if balances.is_empty() {
            balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
        }

        let registry = InstrumentRegistry::from_instruments(instruments);
        let mut state = GlobalState::new(registry, balances);
        state.synthetic_half_spread_bps = cfg.half_spread_bps;
        let checks = BacktestChecks;
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
        let mut ohlcv_series = if matches!(fmt, DataFormat::Ohlcv) {
            BarSeries::from_csv_path(&cfg.data_path, default_tick_size()).ok()
        } else {
            None
        };
        let bar_count = ohlcv_series.as_ref().map_or(0, BarSeries::len);
        let mut equity = Vec::with_capacity(bar_count.max(1));
        let mut metrics = RollingMetrics::new();
        let mut intents = Vec::with_capacity(4);
        let inst_ix = 0usize;

        if strategy.uses_tick_replay() && matches!(fmt, DataFormat::Ohlcv) {
            if let Some(series) = ohlcv_series.take() {
                let tick = series.tick_size();
                let mut src = BarSeriesSource::new(series, exchange.clone(), symbol.clone());
                let mut bar_ix = 0u64;
                while let Some((bar, ts)) = src.next_bar() {
                    bar_ix += 1;
                    if bar_ix % 64 == 0 && cancelled(&cancel) {
                        return Err(crate::Error::Cancelled);
                    }
                    state.apply_bar(inst_ix, &bar, tick, cfg.half_spread_bps);
                    let ev = src.bar_to_event(&bar, ts);
                    dispatch_replay_sync(&mut state, strategy, &checks, &exec, ev, &mut intents)?;
                    if cfg.record_equity_curve {
                        if let Some(eq) = state.mark_to_market_equity_ix(inst_ix) {
                            metrics.record(eq, cfg.periods_per_year);
                            equity.push(EquityPoint {
                                ts,
                                equity_quote: eq,
                            });
                        }
                    }
                }
                let summary = summarize(equity, cfg.periods_per_year);
                let mut report = report_from_summary(
                    summary,
                    state.fill_count,
                    started.elapsed().as_millis() as u64,
                    state.take_fill_log(),
                );
                if report.max_drawdown == 0.0 {
                    report.max_drawdown = metrics.max_drawdown();
                }
                return Ok(report);
            }
        }

        let mut src = load_source(cfg, exchange, symbol, fmt, ohlcv_series)?;
        let mut bar_ix = 0u64;
        while let Some(ev) = src.next_event() {
            bar_ix += 1;
            if bar_ix % 64 == 0 && cancelled(&cancel) {
                return Err(crate::Error::Cancelled);
            }
            let ts = event_ts(&ev);
            if let Event::Market(ref m) = ev {
                state.apply_market(m);
            }
            dispatch_replay_sync(&mut state, strategy, &checks, &exec, ev, &mut intents)?;
            if cfg.record_equity_curve {
                if let Some(eq) = state.mark_to_market_equity_ix(inst_ix) {
                    metrics.record(eq, cfg.periods_per_year);
                    equity.push(EquityPoint {
                        ts,
                        equity_quote: eq,
                    });
                }
            }
        }

        let summary = summarize(equity, cfg.periods_per_year);
        let mut report = report_from_summary(
            summary,
            state.fill_count,
            started.elapsed().as_millis() as u64,
            state.take_fill_log(),
        );
        if report.max_drawdown == 0.0 {
            report.max_drawdown = metrics.max_drawdown();
        }
        Ok(report)
    }
}

fn cancelled(cancel: &Option<Arc<AtomicBool>>) -> bool {
    cancel.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
}

fn instrument_meta_from_config(cfg: &BacktestConfig) -> InstrumentMeta {
    let (base, quote) = parse_base_quote(&cfg.instrument.symbol, cfg.asset_class);
    if cfg.asset_class == AssetClass::Future {
        InstrumentMeta::future(
            base,
            quote,
            cfg.contract_multiplier.unwrap_or(Decimal::ONE),
            cfg.tick_size.unwrap_or(Decimal::new(25, 2)),
            cfg.lot_size,
            cfg.expiry.clone(),
        )
    } else {
        InstrumentMeta {
            base: Asset::new(base),
            quote: Asset::new(quote),
            asset_class: cfg.asset_class,
            lot_size: cfg.lot_size,
            contract_multiplier: None,
            tick_size: cfg.tick_size,
            expiry: cfg.expiry.clone(),
        }
    }
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
    match ev {
        Event::Market(crate::events::MarketEvent::Trade { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::BookL1 { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::Bar { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::BookL2Snapshot(s)) => s.ts,
        Event::Timer(t) => t.ts,
        _ => time::OffsetDateTime::now_utc(),
    }
}

fn report_from_summary(
    s: PerformanceSummary,
    fill_count: u64,
    wall_time_ms: u64,
    fills: Vec<FillRecord>,
) -> BacktestReport {
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
