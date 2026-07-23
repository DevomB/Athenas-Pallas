//! Sync event dispatch for backtest replay.

use super::commands::{
    apply_account_events, apply_control_sync, process_controls_sync, process_intents_sync,
    record_rejection,
};
use crate::error::Result;
use crate::events::{Event, MarketEvent, OrderIntent, RejectionKind};
use crate::execution::SyncExecutionGateway;
use crate::risk::RiskEngine;
use crate::state::GlobalState;
use crate::strategy::{Strategy, StrategyContext, StrategyControl};
use crate::types::{InstrumentId, TradingState};
use time::OffsetDateTime;
use tracing::warn;

/// Sync event dispatch for backtest replay (no async executor on the hot path).
pub fn dispatch_event_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &E,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    process_event_sync(state, strategy, risk, exec, ev, intents)
}

fn process_event_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &E,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    if matches!(&ev, Event::Market(MarketEvent::Bar { .. })) {
        return dispatch_replay_sync(state, strategy, risk, exec, ev, intents);
    }

    match &ev {
        Event::Market(_) => process_live_market_sync(state, risk, exec, &ev, intents)?,
        Event::Account(a) => state.apply_account(a),
        Event::Control(c) => apply_control_sync(state, exec, risk, c)?,
        Event::Timer(_) => {}
    }

    let mut submitted = Vec::new();
    dispatch_strategy_sync(state, strategy, risk, exec, &ev, &mut submitted)
}

fn process_live_market_sync(
    state: &mut GlobalState,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    event: &Event,
    pending: &mut Vec<OrderIntent>,
) -> Result<()> {
    let Event::Market(market) = event else {
        return Ok(());
    };
    state.apply_market(market);
    if let Some(instrument) = event.instrument() {
        let mut ready = Vec::new();
        process_pending_intents_for_instrument_sync(
            state, risk, exec, instrument, pending, &mut ready,
        );
    }
    poll_market_sync(state, exec, event.instrument())
}

/// Run strategy, risk, and execution against live state (no snapshot clones).
pub fn dispatch_strategy_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &E,
    ev: &Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    let now = event_time(ev);
    state.refresh_daily_risk_anchor(now);

    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }

    let ctx = StrategyContext { now, state };
    intents.clear();
    strategy.on_event(&ctx, ev, intents);
    let mut controls = Vec::new();
    strategy.drain_controls(&mut controls);
    process_controls_sync(state, exec, risk, &mut controls)?;
    process_intents_sync(state, risk, exec, intents);

    Ok(())
}

/// Backtest replay: market already applied; inline risk checks.
pub fn dispatch_replay_sync<S>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
{
    if let Event::Market(
        market @ MarketEvent::Bar {
            instrument,
            ts,
            open,
            ..
        },
    ) = &ev
    {
        state.apply_bar_event_open(instrument, *ts, *open);
        let mut ready = Vec::new();
        process_pending_intents_for_instrument_sync(
            state, risk, exec, instrument, intents, &mut ready,
        );
        state.apply_market(market);
        poll_replay_market_instrument_sync(state, exec, instrument)?;
        let mut submitted = Vec::new();
        collect_replay_event_intents_sync(
            state,
            strategy,
            risk,
            exec,
            &ev,
            &mut submitted,
            intents,
        )?;
        return Ok(());
    }

    match &ev {
        Event::Market(_) => poll_market_sync(state, exec, ev.instrument())?,
        Event::Account(a) => state.apply_account(a),
        Event::Control(_) | Event::Timer(_) => {}
    }

    dispatch_strategy_sync(state, strategy, risk, exec, &ev, intents)
}

fn poll_market_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    instrument: Option<&InstrumentId>,
) -> Result<()> {
    let events = match instrument {
        Some(instrument) => exec.poll_after_market_instrument(state, instrument)?,
        None => exec.poll_after_market(state)?,
    };
    apply_account_events(state, events);
    Ok(())
}

/// Run a bar strategy after the completed bar is visible, retaining accepted orders for the next
/// market update of their target instrument.
pub(crate) fn collect_replay_bar_intents_sync<S: Strategy>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    event: &crate::events::ReplayEvent<'_>,
    scratch: &mut Vec<OrderIntent>,
    pending: &mut Vec<OrderIntent>,
) -> Result<()> {
    let now = event.timestamp();
    state.refresh_daily_risk_anchor(now);
    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }
    let ctx = StrategyContext { now, state };
    scratch.clear();
    strategy.on_replay_event(&ctx, event, scratch);
    let mut controls = Vec::new();
    strategy.drain_controls(&mut controls);
    process_controls_sync(state, exec, risk, &mut controls)?;
    for intent in scratch.drain(..) {
        if let Err(error) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {error}");
            record_rejection(state, RejectionKind::Risk, &intent, error.to_string());
        } else {
            pending.push(intent);
        }
    }
    Ok(())
}

/// Owned-event counterpart to [`collect_replay_bar_intents_sync`].
pub(crate) fn collect_replay_event_intents_sync<S: Strategy>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    event: &Event,
    scratch: &mut Vec<OrderIntent>,
    pending: &mut Vec<OrderIntent>,
) -> Result<()> {
    let now = event_time(event);
    state.refresh_daily_risk_anchor(now);
    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }
    let ctx = StrategyContext { now, state };
    scratch.clear();
    strategy.on_event(&ctx, event, scratch);
    let mut controls = Vec::new();
    strategy.drain_controls(&mut controls);
    process_controls_sync(state, exec, risk, &mut controls)?;
    for intent in scratch.drain(..) {
        if let Err(error) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {error}");
            record_rejection(state, RejectionKind::Risk, &intent, error.to_string());
        } else {
            pending.push(intent);
        }
    }
    Ok(())
}

/// Execute prior-bar orders for one instrument against its next observable market state.
pub(crate) fn process_pending_intents_for_instrument_sync(
    state: &mut GlobalState,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    instrument: &InstrumentId,
    pending: &mut Vec<OrderIntent>,
    ready: &mut Vec<OrderIntent>,
) {
    ready.clear();
    for index in (0..pending.len()).rev() {
        if pending[index].instrument == *instrument {
            ready.push(pending.remove(index));
        }
    }
    ready.reverse();
    process_intents_sync(state, risk, exec, ready);
}

/// Apply passive fills for the instrument whose market state just changed.
pub(crate) fn poll_replay_market_instrument_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    instrument: &InstrumentId,
) -> Result<()> {
    poll_market_sync(state, exec, Some(instrument))
}

/// Give a strategy a final callback, then apply its final intents and controls.
pub(crate) fn finalize_strategy_sync<S: Strategy>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    intents: &mut Vec<OrderIntent>,
) -> Result<(bool, bool)> {
    let before = (state.fill_count, state.open_orders.len());
    let now = state.last_event_ts.unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let ctx = StrategyContext { now, state };
    intents.clear();
    strategy.on_finish(&ctx, intents);
    process_intents_sync(state, risk, exec, intents);
    let mut controls = Vec::new();
    strategy.drain_controls(&mut controls);
    let cancel_deferred = controls.iter().any(|control| {
        matches!(
            control,
            StrategyControl::CancelAll | StrategyControl::Flatten
        )
    });
    process_controls_sync(state, exec, risk, &mut controls)?;
    Ok((
        before != (state.fill_count, state.open_orders.len()),
        cancel_deferred,
    ))
}

pub(crate) fn event_time(ev: &Event) -> OffsetDateTime {
    ev.timestamp_or_now()
}
