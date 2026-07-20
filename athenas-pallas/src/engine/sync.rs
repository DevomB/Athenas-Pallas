//! Sync event dispatch for backtest replay.

use crate::error::Result;
use crate::events::{
    AccountEvent, ControlEvent, Event, OrderIntent, OrderIntentSource, RejectionKind,
    RejectionRecord,
};
use crate::execution::SyncExecutionGateway;
use crate::risk::RiskEngine;
use crate::state::{GlobalState, InstrumentIndex};
use crate::strategy::{Strategy, StrategyContext, StrategyControl};
use crate::types::{InstrumentId, OrderType, Side, TradingState};
use rust_decimal::Decimal;
use time::OffsetDateTime;
use tracing::{error, warn};

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
    if let Event::Market(crate::events::MarketEvent::Bar {
        instrument,
        ts,
        open,
        ..
    }) = &ev
    {
        state.apply_bar_event_open(instrument, *ts, *open);
        let mut ready = Vec::new();
        process_pending_intents_for_instrument_sync(
            state, risk, exec, instrument, intents, &mut ready,
        );
        if let Event::Market(market) = &ev {
            state.apply_market(market);
        }
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
        Event::Market(m) => {
            state.apply_market(m);
            if let Some(instrument) = ev.instrument() {
                let mut ready = Vec::new();
                process_pending_intents_for_instrument_sync(
                    state, risk, exec, instrument, intents, &mut ready,
                );
            }
            let passive = match ev.instrument() {
                Some(inst) => exec.poll_after_market_instrument(state, inst)?,
                None => exec.poll_after_market(state)?,
            };
            for a in passive {
                state.apply_account(&a);
            }
        }
        Event::Account(a) => state.apply_account(a),
        Event::Control(c) => apply_control_sync(state, exec, risk, c)?,
        Event::Timer(_) => {}
    }

    let mut submitted = Vec::new();
    dispatch_strategy_sync(state, strategy, risk, exec, &ev, &mut submitted)
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
    if let Event::Market(crate::events::MarketEvent::Bar {
        instrument,
        ts,
        open,
        ..
    }) = &ev
    {
        state.apply_bar_event_open(instrument, *ts, *open);
        let mut ready = Vec::new();
        process_pending_intents_for_instrument_sync(
            state, risk, exec, instrument, intents, &mut ready,
        );
        if let Event::Market(market) = &ev {
            state.apply_market(market);
        }
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
        Event::Market(_) => {
            let passive = match ev.instrument() {
                Some(inst) => exec.poll_after_market_instrument(state, inst)?,
                None => exec.poll_after_market(state)?,
            };
            for a in passive {
                state.apply_account(&a);
            }
        }
        Event::Account(a) => state.apply_account(a),
        Event::Control(_) | Event::Timer(_) => {}
    }

    let now = event_time(&ev);
    state.refresh_daily_risk_anchor(now);

    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }

    let ctx = StrategyContext { now, state };
    intents.clear();
    strategy.on_event(&ctx, &ev, intents);
    let mut controls = Vec::new();
    strategy.drain_controls(&mut controls);
    process_controls_sync(state, exec, risk, &mut controls)?;
    process_intents_sync(state, risk, exec, intents);
    Ok(())
}

/// Zero-allocation tick-replay dispatch using a borrowed [`ReplayEvent`].
pub fn dispatch_replay_bar_sync<S>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    ev: &crate::events::ReplayEvent<'_>,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
{
    let mut ready = Vec::new();
    process_pending_intents_for_instrument_sync(
        state,
        risk,
        exec,
        ev.instrument(),
        intents,
        &mut ready,
    );
    let passive = exec.poll_after_market_instrument(state, ev.instrument())?;
    for a in passive {
        state.apply_account(&a);
    }

    let now = ev.timestamp();
    state.refresh_daily_risk_anchor(now);

    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }

    let mut submitted = Vec::new();
    collect_replay_bar_intents_sync(state, strategy, risk, exec, ev, &mut submitted, intents)
}

pub(crate) fn process_intents_sync(
    state: &mut GlobalState,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    intents: &mut Vec<OrderIntent>,
) {
    for intent in intents.drain(..) {
        if let Err(e) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {e}");
            record_rejection(state, RejectionKind::Risk, &intent, e.to_string());
            continue;
        }
        match dispatch_intent_sync(exec, state, &intent) {
            Ok(evs) => {
                for a in evs {
                    state.apply_account(&a);
                }
            }
            Err(e) => {
                error!(target: "athenas_pallas::engine", "execution: {e}");
                record_rejection(state, RejectionKind::Execution, &intent, e.to_string());
            }
        }
    }
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
    let events = exec.poll_after_market_instrument(state, instrument)?;
    for event in events {
        state.apply_account(&event);
    }
    Ok(())
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

fn record_rejection(
    state: &mut GlobalState,
    kind: RejectionKind,
    intent: &OrderIntent,
    reason: String,
) {
    state.apply_account(&AccountEvent::Rejection(RejectionRecord {
        ts: state.last_event_ts.unwrap_or(OffsetDateTime::UNIX_EPOCH),
        kind,
        instrument: intent.instrument.clone(),
        client_order_id: intent.client_order_id.clone(),
        reason,
    }));
}

fn process_controls_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    risk: &RiskEngine,
    controls: &mut Vec<StrategyControl>,
) -> Result<()> {
    for control in controls.drain(..) {
        match control {
            StrategyControl::CancelOrder(order_id) => {
                let events = exec.cancel(state, order_id)?;
                for event in events {
                    state.apply_account(&event);
                }
            }
            StrategyControl::CancelClientOrder(client_id) => {
                let order_ids: Vec<_> = state
                    .open_orders
                    .values()
                    .filter(|order| order.client_order_id.as_ref() == Some(&client_id))
                    .map(|order| order.id.clone())
                    .collect();
                if order_ids.is_empty() {
                    return Err(crate::Error::Invalid(format!(
                        "unknown client order id {}",
                        client_id.0
                    )));
                }
                for order_id in order_ids {
                    let events = exec.cancel(state, order_id)?;
                    for event in events {
                        state.apply_account(&event);
                    }
                }
            }
            StrategyControl::CancelAll => {
                apply_control_sync(state, exec, risk, &ControlEvent::CancelAll)?;
            }
            StrategyControl::Flatten => {
                apply_control_sync(state, exec, risk, &ControlEvent::Flatten)?;
            }
        }
    }
    Ok(())
}

fn apply_control_sync<E: SyncExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskEngine,
    c: &ControlEvent,
) -> Result<()> {
    match c {
        ControlEvent::Pause => state.paused = true,
        ControlEvent::Resume => state.paused = false,
        ControlEvent::DisableTrading => state.trading_state = TradingState::Disabled,
        ControlEvent::EnableTrading => state.trading_state = TradingState::Enabled,
        ControlEvent::CancelAll => {
            let evs = exec.cancel_all(&state.snapshot())?;
            for a in evs {
                state.apply_account(&a);
            }
        }
        ControlEvent::Flatten => {
            let evs = exec.cancel_all(&state.snapshot())?;
            for a in evs {
                state.apply_account(&a);
            }
            let insts: Vec<_> = state
                .positions
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.is_zero())
                .filter_map(|(ix, _)| state.registry.id(InstrumentIndex(ix)).cloned())
                .collect();
            for inst in insts {
                close_position_with_flatten_source_sync(state, exec, risk, inst);
            }
        }
    }
    Ok(())
}

fn close_position_with_flatten_source_sync<E: SyncExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskEngine,
    inst: InstrumentId,
) {
    let snap = state.snapshot();
    let pos = snap.position_qty(&inst);
    if pos.is_zero() {
        return;
    }
    let intent = OrderIntent {
        instrument: inst,
        side: if pos > Decimal::ZERO {
            Side::Sell
        } else {
            Side::Buy
        },
        order_type: OrderType::Market,
        price: None,
        stop_price: None,
        qty: pos.abs(),
        client_order_id: None,
        oco_group: None,
        source: OrderIntentSource::Flatten,
        strategy_id: None,
    };
    if let Err(e) = risk.validate(&snap, &intent) {
        warn!(target: "athenas_pallas::engine", "flatten risk: {e}");
        record_rejection(state, RejectionKind::Risk, &intent, e.to_string());
        return;
    }
    match dispatch_intent_sync(exec, &snap, &intent) {
        Ok(evs) => {
            for a in evs {
                state.apply_account(&a);
            }
        }
        Err(e) => {
            error!(target: "athenas_pallas::engine", "flatten execution: {e}");
            record_rejection(state, RejectionKind::Execution, &intent, e.to_string());
        }
    }
}

fn dispatch_intent_sync<E: SyncExecutionGateway>(
    exec: &E,
    state: &GlobalState,
    intent: &OrderIntent,
) -> Result<crate::execution::AccountEvents> {
    match intent.order_type {
        OrderType::Limit => exec.place_limit(state, intent),
        OrderType::Market => exec.place_market(state, intent),
        OrderType::StopMarket => exec.place_stop_market(state, intent),
        OrderType::StopLimit => exec.place_stop_limit(state, intent),
    }
}

pub(crate) fn event_time(ev: &Event) -> OffsetDateTime {
    ev.timestamp_or_now()
}
