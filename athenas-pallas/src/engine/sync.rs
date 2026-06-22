//! Sync event dispatch for backtest replay.

use crate::error::Result;
use crate::events::{ControlEvent, Event, OrderIntent, OrderIntentSource};
use crate::execution::SyncExecutionGateway;
use crate::risk::RiskPipeline;
use crate::state::{GlobalState, InstrumentIndex};
use crate::strategy::{Strategy, StrategyContext};
use crate::types::{InstrumentId, OrderType, Side, TradingState};
use rust_decimal::Decimal;
use time::OffsetDateTime;
use tracing::{error, warn};

/// Sync event dispatch for backtest replay (no async executor on the hot path).
pub fn dispatch_event_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
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
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    match &ev {
        Event::Market(m) => {
            state.apply_market(m);
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

    dispatch_strategy_sync(state, strategy, risk, exec, &ev, intents)
}

/// Run strategy, risk, and execution against live state (no snapshot clones).
pub fn dispatch_strategy_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
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

    for intent in intents.drain(..) {
        if let Err(e) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {e}");
            continue;
        }
        match dispatch_intent_sync(exec, state, &intent) {
            Ok(evs) => {
                for a in evs {
                    state.apply_account(&a);
                }
            }
            Err(e) => error!(target: "athenas_pallas::engine", "execution: {e}"),
        }
    }

    Ok(())
}

/// Backtest replay: market already applied; inline risk checks.
pub fn dispatch_replay_sync<S>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &crate::risk::BacktestChecks,
    exec: &impl SyncExecutionGateway,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
{
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

    process_intents_sync(state, risk, exec, intents);
    Ok(())
}

/// Zero-allocation tick-replay dispatch using a borrowed [`ReplayEvent`].
pub fn dispatch_replay_bar_sync<S>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &crate::risk::BacktestChecks,
    exec: &impl SyncExecutionGateway,
    ev: &crate::events::ReplayEvent<'_>,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
{
    let passive = exec.poll_after_market_instrument(state, ev.instrument())?;
    for a in passive {
        state.apply_account(&a);
    }

    let now = ev.timestamp();
    state.refresh_daily_risk_anchor(now);

    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }

    let ctx = StrategyContext { now, state };
    intents.clear();
    strategy.on_replay_event(&ctx, ev, intents);

    process_intents_sync(state, risk, exec, intents);
    Ok(())
}

fn process_intents_sync(
    state: &mut GlobalState,
    risk: &crate::risk::BacktestChecks,
    exec: &impl SyncExecutionGateway,
    intents: &mut Vec<OrderIntent>,
) {
    for intent in intents.drain(..) {
        if let Err(e) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {e}");
            continue;
        }
        match dispatch_intent_sync(exec, state, &intent) {
            Ok(evs) => {
                for a in evs {
                    state.apply_account(&a);
                }
            }
            Err(e) => error!(target: "athenas_pallas::engine", "execution: {e}"),
        }
    }
}

fn apply_control_sync<E: SyncExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskPipeline,
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
    risk: &RiskPipeline,
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
        source: OrderIntentSource::Flatten,
        strategy_id: None,
    };
    if let Err(e) = risk.validate(&snap, &intent) {
        warn!(target: "athenas_pallas::engine", "flatten risk: {e}");
        return;
    }
    match dispatch_intent_sync(exec, &snap, &intent) {
        Ok(evs) => {
            for a in evs {
                state.apply_account(&a);
            }
        }
        Err(e) => error!(target: "athenas_pallas::engine", "flatten execution: {e}"),
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
