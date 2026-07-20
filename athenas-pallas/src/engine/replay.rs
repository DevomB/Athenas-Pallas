//! Sync replay of recorded event streams.

use crate::error::Result;
use crate::events::Event;
use crate::execution::SyncExecutionGateway;
use crate::risk::RiskEngine;
use crate::state::GlobalState;
use crate::strategy::Strategy;

use super::sync::dispatch_event_sync;

/// Replay events through the sync dispatch path (replaces async batch replay).
pub fn replay_events_sync<S, E>(
    mut state: GlobalState,
    mut strategy: S,
    risk: &RiskEngine,
    exec: &E,
    events: Vec<Event>,
) -> Result<GlobalState>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    let mut intents = Vec::new();
    for ev in events {
        dispatch_event_sync(&mut state, &mut strategy, risk, exec, ev, &mut intents)?;
    }
    Ok(state)
}
