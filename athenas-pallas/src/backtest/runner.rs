//! Synchronous backtest driver.

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::config::{
    instrument_meta_from_config, instrument_meta_from_extra, BacktestConfig, DataFormat,
};
use super::lifecycle::apply_bar_lifecycle;
use super::merge::merge_sources_iter;
use super::report::{
    report_from_summary, BacktestParameters, BacktestReport, DataMetadata, DataSourceMetadata,
    FinalPosition, PendingOrder, ReportDetails,
};
use super::source_loader::{load_all_sources, load_source, resolve_format};
use crate::bar::{default_tick_size, BarSeries, BarSeriesSource};
use crate::calendar::{is_session_open, SessionFilter};
use crate::dispatch_replay_sync;
use crate::engine::{
    collect_replay_bar_intents_sync, collect_replay_event_intents_sync, finalize_strategy_sync,
    poll_replay_market_instrument_sync, process_pending_intents_for_instrument_sync,
};
use crate::events::{Event, FillRecord, MarketEvent, OrderIntent};
use crate::execution::{PaperConfig, PaperExecution};
use crate::interval::periods_per_year_from_interval_for_class;
use crate::metrics::{summarize_with_fills_and_rf, RollingMetrics};
use crate::risk::{MaxDailyLossQuote, MaxPositionSize, RiskEngine};
use crate::state::{GlobalState, InstrumentRegistry};
use crate::strategy::{Strategy, StrategyContext};
use crate::types::{EquityPoint, ExchangeId, InstrumentId, OrderType, Side, Symbol};

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
            oco_group: None,
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
        let meta = instrument_meta_from_config(cfg);
        let qty = cfg.buy_and_hold_qty.unwrap_or_else(|| {
            meta.lot_size.unwrap_or_else(|| {
                if meta.asset_class == crate::instrument::AssetClass::Crypto {
                    Decimal::new(1, 2)
                } else {
                    Decimal::ONE
                }
            })
        });
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
        let mut run = ReplayRun::new(cfg)?;
        if run.can_borrow_bars(strategy) {
            if let Some(series) = run.series.take() {
                run.replay_bars(series, strategy, cancel.as_ref())?;
            }
        } else {
            let series = run.series.take();
            if run.multi_instrument {
                let mut sources = load_all_sources(
                    cfg,
                    run.exchange.clone(),
                    run.symbol.clone(),
                    run.format,
                    series,
                )?;
                run.replay_events(merge_sources_iter(&mut sources), strategy, cancel.as_ref())?;
            } else {
                let mut source = load_source(
                    cfg,
                    run.exchange.clone(),
                    run.symbol.clone(),
                    run.format,
                    series,
                )?;
                run.replay_events(
                    std::iter::from_fn(|| source.next_event()),
                    strategy,
                    cancel.as_ref(),
                )?;
            }
        }
        run.finish_strategy(strategy)?;
        Ok(run.finish())
    }
}

struct ReplayRun<'a> {
    cfg: &'a BacktestConfig,
    session: SessionFilter,
    state: GlobalState,
    risk: RiskEngine,
    execution: PaperExecution,
    exchange: ExchangeId,
    symbol: Symbol,
    format: DataFormat,
    series: Option<BarSeries>,
    multi_instrument: bool,
    periods_per_year: f64,
    equity: Vec<EquityPoint>,
    metrics: RollingMetrics,
    intents: Vec<OrderIntent>,
    pending_bar_intents: Vec<OrderIntent>,
    ready_intents: Vec<OrderIntent>,
    primary_ix: usize,
    processed_events: u64,
    first_event_ts: Option<time::OffsetDateTime>,
    last_event_ts: Option<time::OffsetDateTime>,
    started: Instant,
}

impl<'a> ReplayRun<'a> {
    fn new(cfg: &'a BacktestConfig) -> crate::Result<Self> {
        let meta = instrument_meta_from_config(cfg);
        let mut instruments = HashMap::new();
        instruments.insert(cfg.instrument.clone(), meta.clone());
        instruments.extend(
            cfg.extra_instruments
                .iter()
                .map(|extra| (extra.instrument.clone(), instrument_meta_from_extra(extra))),
        );
        let balances = if cfg.balances.is_empty() {
            cfg.default_balances()
        } else {
            cfg.balances.clone()
        };
        let mut state =
            GlobalState::new(InstrumentRegistry::from_instruments(instruments), balances);
        let primary_ix = state
            .registry
            .index_of(&cfg.instrument)
            .expect("primary instrument was inserted")
            .0;
        state.synthetic_half_spread_bps = cfg.half_spread_bps;
        let risk = configure_risk(cfg, &meta, &mut state);
        let format = resolve_format(&cfg.data_path, cfg.data_format)?;
        let series = load_bar_series(cfg, format)?;
        let periods_per_year = resolve_periods_per_year(cfg, series.as_ref());
        let capacity = series.as_ref().map_or(1, BarSeries::len);
        Ok(Self {
            cfg,
            session: cfg
                .session_filter
                .as_deref()
                .map(SessionFilter::parse)
                .unwrap_or_default(),
            state,
            risk,
            execution: PaperExecution::new(PaperConfig {
                fee_bps: cfg.fee_bps,
                market_slippage_bps: cfg.slippage_bps,
                ..PaperConfig::default()
            }),
            exchange: ExchangeId::new(cfg.instrument.exchange.as_str()),
            symbol: Symbol::new(cfg.instrument.symbol.as_str()),
            format,
            series,
            multi_instrument: cfg.extra_instruments.iter().any(|e| e.data_path.is_some()),
            periods_per_year,
            equity: if cfg.record_equity_curve {
                Vec::with_capacity(capacity)
            } else {
                Vec::new()
            },
            metrics: RollingMetrics::new(),
            intents: Vec::with_capacity(4),
            pending_bar_intents: Vec::with_capacity(4),
            ready_intents: Vec::with_capacity(4),
            primary_ix,
            processed_events: 0,
            first_event_ts: None,
            last_event_ts: None,
            started: Instant::now(),
        })
    }

    fn can_borrow_bars(&self, strategy: &impl Strategy) -> bool {
        strategy.uses_tick_replay() && self.format == DataFormat::Ohlcv && !self.multi_instrument
    }

    fn replay_bars<S: Strategy>(
        &mut self,
        series: BarSeries,
        strategy: &mut S,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> crate::Result<()> {
        let tick = series.tick_size();
        let mut source = BarSeriesSource::new(series, self.exchange.clone(), self.symbol.clone());
        let mut bar_ix = 0;
        while let Some((bar, ts)) = source.next_bar() {
            if !self.accept(ts, &mut bar_ix, cancel)? {
                continue;
            }
            self.track_event(ts);
            self.state
                .apply_bar_open(self.primary_ix, &bar, tick, self.cfg.half_spread_bps);
            process_pending_intents_for_instrument_sync(
                &mut self.state,
                &self.risk,
                &self.execution,
                &self.cfg.instrument,
                &mut self.pending_bar_intents,
                &mut self.ready_intents,
            );
            self.state
                .apply_bar(self.primary_ix, &bar, tick, self.cfg.half_spread_bps);
            self.state.refresh_daily_risk_anchor(ts);
            let event = source.bar_to_replay_event(&bar, ts);
            poll_replay_market_instrument_sync(
                &mut self.state,
                &self.execution,
                &self.cfg.instrument,
            )?;
            collect_replay_bar_intents_sync(
                &mut self.state,
                strategy,
                &self.risk,
                &self.execution,
                &event,
                &mut self.intents,
                &mut self.pending_bar_intents,
            )?;
            apply_bar_lifecycle(&mut self.state, ts);
            self.record_equity(ts);
        }
        Ok(())
    }

    fn replay_events<S: Strategy>(
        &mut self,
        events: impl Iterator<Item = Event>,
        strategy: &mut S,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> crate::Result<()> {
        let mut event_ix = 0;
        for event in events {
            let ts = event_ts(&event);
            if !self.accept(ts, &mut event_ix, cancel)? {
                continue;
            }
            self.track_event(ts);
            if let Event::Market(MarketEvent::Bar {
                instrument, open, ..
            }) = &event
            {
                self.state.apply_bar_event_open(instrument, ts, *open);
                process_pending_intents_for_instrument_sync(
                    &mut self.state,
                    &self.risk,
                    &self.execution,
                    instrument,
                    &mut self.pending_bar_intents,
                    &mut self.ready_intents,
                );
                if let Event::Market(market) = &event {
                    self.state.apply_market(market);
                }
                self.state.refresh_daily_risk_anchor(ts);
                poll_replay_market_instrument_sync(&mut self.state, &self.execution, instrument)?;
                collect_replay_event_intents_sync(
                    &mut self.state,
                    strategy,
                    &self.risk,
                    &self.execution,
                    &event,
                    &mut self.intents,
                    &mut self.pending_bar_intents,
                )?;
                apply_bar_lifecycle(&mut self.state, ts);
                self.record_equity(ts);
                continue;
            }
            if let Event::Market(ref market) = event {
                self.state.apply_market(market);
                self.state.refresh_daily_risk_anchor(ts);
                if let Some(instrument) = event.instrument() {
                    process_pending_intents_for_instrument_sync(
                        &mut self.state,
                        &self.risk,
                        &self.execution,
                        instrument,
                        &mut self.pending_bar_intents,
                        &mut self.ready_intents,
                    );
                }
            }
            dispatch_replay_sync(
                &mut self.state,
                strategy,
                &self.risk,
                &self.execution,
                event,
                &mut self.intents,
            )?;
            self.record_equity(ts);
        }
        Ok(())
    }

    fn track_event(&mut self, ts: time::OffsetDateTime) {
        self.processed_events += 1;
        self.first_event_ts.get_or_insert(ts);
        self.last_event_ts = Some(ts);
    }

    fn finish_strategy<S: Strategy>(&mut self, strategy: &mut S) -> crate::Result<()> {
        let (changed, cancel_deferred) = finalize_strategy_sync(
            &mut self.state,
            strategy,
            &self.risk,
            &self.execution,
            &mut self.intents,
        )?;
        if cancel_deferred {
            self.pending_bar_intents.clear();
        }
        if changed {
            if let Some(ts) = self.last_event_ts {
                self.record_equity(ts);
            }
        }
        Ok(())
    }

    fn accept(
        &self,
        ts: time::OffsetDateTime,
        event_ix: &mut u64,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> crate::Result<bool> {
        if !is_session_open(self.session, ts) {
            return Ok(false);
        }
        *event_ix += 1;
        if *event_ix % 64 == 0 {
            if let Some(hook) = &self.cfg.on_progress {
                hook(&format!("processed {event_ix} bars"));
            }
            if cancelled(cancel) {
                return Err(crate::Error::Cancelled);
            }
        }
        Ok(true)
    }

    fn record_equity(&mut self, ts: time::OffsetDateTime) {
        let Some(equity) = record_equity(&self.state, self.multi_instrument, self.primary_ix)
        else {
            return;
        };
        self.metrics.record(equity, self.periods_per_year);
        if self.cfg.record_equity_curve {
            self.equity.push(EquityPoint {
                ts,
                equity_quote: equity,
            });
        }
    }

    fn finish(mut self) -> BacktestReport {
        let fills = self.state.take_fill_log();
        let details = build_report_details(
            self.cfg,
            self.format,
            self.processed_events,
            self.first_event_ts,
            self.last_event_ts,
            &self.state,
            &self.pending_bar_intents,
            fills,
        );
        finalize_report(
            self.cfg,
            self.equity,
            &self.metrics,
            self.periods_per_year,
            details,
            self.started,
        )
    }
}

fn configure_risk(
    cfg: &BacktestConfig,
    meta: &crate::instrument::InstrumentMeta,
    state: &mut GlobalState,
) -> RiskEngine {
    let mut risk = RiskEngine::default();
    if let Some(max_abs) = cfg.max_position_abs {
        risk = risk.with_max_position(MaxPositionSize {
            instrument: cfg.instrument.clone(),
            max_abs,
        });
    }
    if let Some(max_loss) = cfg.max_daily_loss_quote {
        let quote = meta.quote.clone();
        state.daily_risk_quote = Some(quote.clone());
        risk = risk.with_max_daily_loss(MaxDailyLossQuote { quote, max_loss });
    }
    risk
}

fn load_bar_series(cfg: &BacktestConfig, format: DataFormat) -> crate::Result<Option<BarSeries>> {
    if format == DataFormat::Ohlcv {
        BarSeries::from_csv_path_or_pbar(&cfg.data_path, default_tick_size())
            .map(Some)
            .map_err(crate::Error::Io)
    } else {
        Ok(None)
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
    mut details: ReportDetails,
    started: Instant,
) -> BacktestReport {
    details.parameters.periods_per_year = periods_per_year;
    let fills = &details.fills;
    let summary = if cfg.record_equity_curve {
        summarize_with_fills_and_rf(equity, periods_per_year, fills, cfg.risk_free_annual)
    } else {
        metrics.streaming_summary(periods_per_year, fills, cfg.risk_free_annual)
    };
    let mut report = report_from_summary(summary, started.elapsed().as_millis() as u64, details);
    if report.max_drawdown == 0.0 {
        report.max_drawdown = metrics.max_drawdown();
    }
    report
}

#[allow(clippy::too_many_arguments)]
fn build_report_details(
    cfg: &BacktestConfig,
    format: DataFormat,
    processed_events: u64,
    start: Option<time::OffsetDateTime>,
    end: Option<time::OffsetDateTime>,
    state: &GlobalState,
    deferred: &[OrderIntent],
    fills: Vec<FillRecord>,
) -> ReportDetails {
    let (total_fees, turnover) = report_costs(&fills, state);
    ReportDetails {
        parameters: report_parameters(cfg),
        data: report_data(cfg, format, processed_events, start, end),
        fills,
        total_fees: total_fees.to_string(),
        turnover: turnover.to_string(),
        risk_rejection_count: state.risk_rejection_count,
        execution_rejection_count: state.execution_rejection_count,
        rejections: state.rejection_log.clone(),
        pending_orders: report_pending_orders(state, deferred),
        final_positions: report_final_positions(state),
    }
}

fn report_parameters(cfg: &BacktestConfig) -> BacktestParameters {
    let balances = if cfg.balances.is_empty() {
        cfg.default_balances()
    } else {
        cfg.balances.clone()
    };
    BacktestParameters {
        fee_bps: cfg.fee_bps.to_string(),
        slippage_bps: cfg.slippage_bps.to_string(),
        half_spread_bps: cfg.half_spread_bps.to_string(),
        buy_and_hold_qty: cfg.buy_and_hold_qty.map(|value| value.to_string()),
        periods_per_year: cfg.periods_per_year,
        bar_interval: cfg.bar_interval.clone(),
        session_filter: cfg.session_filter.clone(),
        risk_free_annual: cfg.risk_free_annual,
        max_position_abs: cfg.max_position_abs.map(|value| value.to_string()),
        max_daily_loss_quote: cfg.max_daily_loss_quote.map(|value| value.to_string()),
        margin_initial_rate: cfg.margin_initial_rate.map(|value| value.to_string()),
        record_equity_curve: cfg.record_equity_curve,
        strategy_path: cfg
            .strategy_path
            .as_ref()
            .map(|path| path.display().to_string()),
        strategy_parameters: cfg.strategy_parameters.clone(),
        initial_balances: balances
            .into_iter()
            .map(|(asset, amount)| (asset.to_string(), amount.to_string()))
            .collect(),
    }
}

fn report_data(
    cfg: &BacktestConfig,
    format: DataFormat,
    processed_events: u64,
    start: Option<time::OffsetDateTime>,
    end: Option<time::OffsetDateTime>,
) -> DataMetadata {
    let mut sources = vec![DataSourceMetadata {
        instrument: cfg.instrument.clone(),
        path: Some(cfg.data_path.display().to_string()),
        format: data_format_name(format).into(),
    }];
    sources.extend(cfg.extra_instruments.iter().map(|extra| {
        let configured = extra.data_format.unwrap_or(DataFormat::Auto);
        let format = extra
            .data_path
            .as_deref()
            .and_then(|path| resolve_format(path, configured).ok())
            .unwrap_or(configured);
        DataSourceMetadata {
            instrument: extra.instrument.clone(),
            path: extra
                .data_path
                .as_ref()
                .map(|path| path.display().to_string()),
            format: data_format_name(format).into(),
        }
    }));
    DataMetadata {
        sources,
        processed_events,
        start,
        end,
    }
}

fn report_costs(fills: &[FillRecord], state: &GlobalState) -> (Decimal, Decimal) {
    let total_fees = fills
        .iter()
        .filter_map(|fill| fill.fee.parse::<Decimal>().ok())
        .sum::<Decimal>();
    let turnover = fills
        .iter()
        .filter_map(|fill| {
            let price = fill.price.parse::<Decimal>().ok()?;
            let qty = fill.qty.parse::<Decimal>().ok()?;
            let multiplier = state
                .registry
                .meta_by_id(&fill.instrument)
                .and_then(|meta| meta.contract_multiplier)
                .unwrap_or(Decimal::ONE);
            Some(price.abs() * qty.abs() * multiplier)
        })
        .sum::<Decimal>();
    (total_fees, turnover)
}

fn report_pending_orders(state: &GlobalState, deferred: &[OrderIntent]) -> Vec<PendingOrder> {
    let mut pending_orders: Vec<_> = state
        .open_orders
        .values()
        .map(|order| PendingOrder {
            order_id: Some(order.id.clone()),
            instrument: order.instrument.clone(),
            side: order.side,
            order_type: order.order_type,
            qty: order.remaining_qty.to_string(),
            price: order.price.map(|value| value.to_string()),
            stop_price: order.stop_price.map(|value| value.to_string()),
            client_order_id: order.client_order_id.clone(),
            oco_group: order.oco_group.clone(),
            strategy_id: order.strategy_id.clone(),
            state: "working".into(),
        })
        .collect();
    pending_orders.extend(deferred.iter().map(|intent| PendingOrder {
        order_id: None,
        instrument: intent.instrument.clone(),
        side: intent.side,
        order_type: intent.order_type,
        qty: intent.qty.to_string(),
        price: intent.price.map(|value| value.to_string()),
        stop_price: intent.stop_price.map(|value| value.to_string()),
        client_order_id: intent.client_order_id.clone(),
        oco_group: intent.oco_group.clone(),
        strategy_id: intent.strategy_id.clone(),
        state: "awaiting_next_market".into(),
    }));
    pending_orders
}

fn report_final_positions(state: &GlobalState) -> Vec<FinalPosition> {
    state
        .registry
        .iter()
        .map(|(index, instrument, _)| FinalPosition {
            instrument: instrument.clone(),
            qty: state.positions[index.0].to_string(),
        })
        .collect()
}

fn data_format_name(format: DataFormat) -> &'static str {
    match format {
        DataFormat::Auto => "auto",
        DataFormat::Ohlcv => "ohlcv",
        DataFormat::Fx => "fx",
    }
}

fn cancelled(cancel: Option<&Arc<AtomicBool>>) -> bool {
    cancel.is_some_and(|f| f.load(Ordering::Relaxed))
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

fn event_ts(ev: &Event) -> time::OffsetDateTime {
    ev.timestamp_or_now()
}
