//! Pluggable strategy interface.

use crate::events::{Event, OrderIntent};
use crate::state::GlobalState;
use time::OffsetDateTime;

/// Read-only context passed to strategies.
pub struct StrategyContext<'a> {
    /// Current engine time (event time or replay clock).
    pub now: OffsetDateTime,
    /// Read-only state.
    pub state: &'a GlobalState,
}

/// User strategy hook.
pub trait Strategy: Send {
    /// React to one normalized event; return order intents (may be empty).
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event) -> Vec<OrderIntent>;
}

/// Run several strategies in order and concatenate their [`OrderIntent`] lists (multi-sleeve / composite).
///
/// Tag each child's intents with a distinct [`crate::types::StrategyId`] on [`OrderIntent::strategy_id`](crate::events::OrderIntent::strategy_id)
/// if you want attributed positions in [`crate::state::GlobalState::strategy_positions`].
pub struct CompositeStrategy {
    /// Child strategies (evaluation order is preserved).
    pub children: Vec<Box<dyn Strategy + Send>>,
}

impl CompositeStrategy {
    /// New composite from owned boxed strategies.
    pub fn new(children: Vec<Box<dyn Strategy + Send>>) -> Self {
        Self { children }
    }
}

impl Strategy for CompositeStrategy {
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event) -> Vec<OrderIntent> {
        let mut out = Vec::new();
        for child in &mut self.children {
            out.extend(child.on_event(ctx, event));
        }
        out
    }
}
