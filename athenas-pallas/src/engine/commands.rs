//! Order intents and strategy controls applied by synchronous dispatch.

use crate::error::Result;
use crate::events::{
    AccountEvent, ControlEvent, OrderIntent, OrderIntentSource, RejectionKind, RejectionRecord,
};
use crate::execution::{AccountEvents, SyncExecutionGateway};
use crate::risk::RiskEngine;
use crate::state::{GlobalState, InstrumentIndex};
use crate::strategy::StrategyControl;
use crate::types::{ClientOrderId, InstrumentId, OrderId, OrderType, Side, TradingState};
use rust_decimal::Decimal;
use time::OffsetDateTime;
use tracing::{error, warn};

pub(super) fn process_intents_sync(
    state: &mut GlobalState,
    risk: &RiskEngine,
    exec: &impl SyncExecutionGateway,
    intents: &mut Vec<OrderIntent>,
) {
    for intent in intents.drain(..) {
        if let Err(error) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {error}");
            record_rejection(state, RejectionKind::Risk, &intent, error.to_string());
            continue;
        }
        match dispatch_intent_sync(exec, state, &intent) {
            Ok(events) => apply_account_events(state, events),
            Err(error) => {
                error!(target: "athenas_pallas::engine", "execution: {error}");
                record_rejection(state, RejectionKind::Execution, &intent, error.to_string());
            }
        }
    }
}

pub(super) fn process_controls_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    risk: &RiskEngine,
    controls: &mut Vec<StrategyControl>,
) -> Result<()> {
    for control in controls.drain(..) {
        match control {
            StrategyControl::CancelOrder(order_id) => cancel_order_sync(state, exec, order_id)?,
            StrategyControl::CancelClientOrder(client_id) => {
                cancel_client_orders(state, exec, &client_id)?
            }
            StrategyControl::CancelAll => {
                apply_control_sync(state, exec, risk, &ControlEvent::CancelAll)?
            }
            StrategyControl::Flatten => {
                apply_control_sync(state, exec, risk, &ControlEvent::Flatten)?
            }
        }
    }
    Ok(())
}

fn cancel_client_orders(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    client_id: &ClientOrderId,
) -> Result<()> {
    let order_ids: Vec<_> = state
        .open_orders
        .values()
        .filter(|order| order.client_order_id.as_ref() == Some(client_id))
        .map(|order| order.id.clone())
        .collect();
    if order_ids.is_empty() {
        return Err(crate::Error::Invalid(format!(
            "unknown client order id {}",
            client_id.0
        )));
    }
    for order_id in order_ids {
        cancel_order_sync(state, exec, order_id)?;
    }
    Ok(())
}

pub(super) fn apply_control_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    risk: &RiskEngine,
    control: &ControlEvent,
) -> Result<()> {
    match control {
        ControlEvent::Pause => state.paused = true,
        ControlEvent::Resume => state.paused = false,
        ControlEvent::DisableTrading => state.trading_state = TradingState::Disabled,
        ControlEvent::EnableTrading => state.trading_state = TradingState::Enabled,
        ControlEvent::CancelAll => cancel_all_sync(state, exec)?,
        ControlEvent::Flatten => {
            cancel_all_sync(state, exec)?;
            flatten_positions(state, exec, risk);
        }
    }
    Ok(())
}

fn flatten_positions(state: &mut GlobalState, exec: &impl SyncExecutionGateway, risk: &RiskEngine) {
    let instruments: Vec<_> = state
        .positions
        .iter()
        .enumerate()
        .filter(|(_, position)| !position.is_zero())
        .filter_map(|(index, _)| state.registry.id(InstrumentIndex(index)).cloned())
        .collect();
    for instrument in instruments {
        close_position_with_flatten_source_sync(state, exec, risk, instrument);
    }
}

fn close_position_with_flatten_source_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    risk: &RiskEngine,
    instrument: InstrumentId,
) {
    let snapshot = state.snapshot();
    let position = snapshot.position_qty(&instrument);
    if position.is_zero() {
        return;
    }
    let intent = OrderIntent {
        instrument,
        side: if position > Decimal::ZERO {
            Side::Sell
        } else {
            Side::Buy
        },
        order_type: OrderType::Market,
        price: None,
        stop_price: None,
        qty: position.abs(),
        client_order_id: None,
        oco_group: None,
        source: OrderIntentSource::Flatten,
        strategy_id: None,
    };
    if let Err(error) = risk.validate(&snapshot, &intent) {
        warn!(target: "athenas_pallas::engine", "flatten risk: {error}");
        record_rejection(state, RejectionKind::Risk, &intent, error.to_string());
        return;
    }
    match dispatch_intent_sync(exec, &snapshot, &intent) {
        Ok(events) => apply_account_events(state, events),
        Err(error) => {
            error!(target: "athenas_pallas::engine", "flatten execution: {error}");
            record_rejection(state, RejectionKind::Execution, &intent, error.to_string());
        }
    }
}

pub(super) fn record_rejection(
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

pub(super) fn apply_account_events(state: &mut GlobalState, events: AccountEvents) {
    for event in events {
        state.apply_account(&event);
    }
}

fn cancel_order_sync(
    state: &mut GlobalState,
    exec: &impl SyncExecutionGateway,
    order_id: OrderId,
) -> Result<()> {
    let events = exec.cancel(state, order_id)?;
    apply_account_events(state, events);
    Ok(())
}

fn cancel_all_sync(state: &mut GlobalState, exec: &impl SyncExecutionGateway) -> Result<()> {
    let events = exec.cancel_all(&state.snapshot())?;
    apply_account_events(state, events);
    Ok(())
}

fn dispatch_intent_sync(
    exec: &impl SyncExecutionGateway,
    state: &GlobalState,
    intent: &OrderIntent,
) -> Result<AccountEvents> {
    match intent.order_type {
        OrderType::Limit => exec.place_limit(state, intent),
        OrderType::Market => exec.place_market(state, intent),
        OrderType::StopMarket => exec.place_stop_market(state, intent),
        OrderType::StopLimit => exec.place_stop_limit(state, intent),
    }
}
