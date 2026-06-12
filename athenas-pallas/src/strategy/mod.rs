//! Pluggable strategy interface.
#![allow(missing_docs)]

pub mod external;
pub mod protocol;

use crate::events::{Event, OrderIntent};
use crate::state::GlobalState;
use time::OffsetDateTime;

pub use external::ExternalStrategy;

/// Read-only context passed to strategies.
pub struct StrategyContext<'a> {
    pub now: OffsetDateTime,
    pub state: &'a GlobalState,
}

/// User strategy hook.
pub trait Strategy: Send {
    /// Append order intents into `out` (cleared by the engine each event).
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event, out: &mut Vec<OrderIntent>);

    /// When true, replay walks [`crate::backtest::BarSeries`] by index instead of allocating per-bar events.
    fn uses_tick_replay(&self) -> bool {
        false
    }
}

/// No-op for benchmarks.
#[derive(Default)]
pub struct NoopStrategy;

impl Strategy for NoopStrategy {
    fn on_event(&mut self, _: &StrategyContext<'_>, _: &Event, _: &mut Vec<OrderIntent>) {}

    fn uses_tick_replay(&self) -> bool {
        true
    }
}

/// Multi-sleeve composite.
pub struct CompositeStrategy {
    pub children: Vec<Box<dyn Strategy + Send>>,
}

impl CompositeStrategy {
    pub fn new(children: Vec<Box<dyn Strategy + Send>>) -> Self {
        Self { children }
    }
}

impl Strategy for CompositeStrategy {
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event, out: &mut Vec<OrderIntent>) {
        for child in &mut self.children {
            child.on_event(ctx, event, out);
        }
    }
}
