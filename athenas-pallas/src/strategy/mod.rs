//! Pluggable strategy interface.
#![allow(missing_docs)]

pub mod external;
pub mod protocol;
pub mod sizing;

use crate::events::{Event, OrderIntent, ReplayEvent};
use crate::state::GlobalState;
use crate::types::{ClientOrderId, InstrumentId, OrderId};
use rust_decimal::Decimal;
use time::OffsetDateTime;

pub use external::ExternalStrategy;
pub use sizing::position_size_pct_equity;

/// Read-only context passed to strategies.
pub struct StrategyContext<'a> {
    pub now: OffsetDateTime,
    pub state: &'a GlobalState,
}

/// Non-placement actions a strategy can request alongside order intents.
#[derive(Clone, Debug)]
pub enum StrategyControl {
    /// Cancel one engine order id.
    CancelOrder(OrderId),
    /// Cancel the working order carrying this client id.
    CancelClientOrder(ClientOrderId),
    /// Cancel every working order.
    CancelAll,
    /// Cancel every working order and close all positions at market.
    Flatten,
}

impl StrategyContext<'_> {
    /// Base quantity for `pct` (0..1) of current mark-to-market equity on `instrument`.
    ///
    /// Returns `Decimal::ZERO` when no price/equity is available. Convenience wrapper around
    /// [`position_size_pct_equity`] using engine state, mirroring the Python/C++ SDK helper.
    pub fn size_pct_equity(&self, instrument: &InstrumentId, pct: Decimal) -> Decimal {
        let Some(mid) = self.state.mid_or_last(instrument) else {
            return Decimal::ZERO;
        };
        let Some(equity) = self.state.mark_to_market_equity(instrument) else {
            return Decimal::ZERO;
        };
        position_size_pct_equity(equity, mid, pct)
    }
}

/// User strategy hook.
pub trait Strategy: Send {
    /// Append order intents into `out` (cleared by the engine each event).
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event, out: &mut Vec<OrderIntent>);

    /// Zero-allocation replay hook for the tick-native fast path.
    ///
    /// The default bridges to [`Strategy::on_event`] by materializing an owned [`Event`]. Override
    /// this to read the borrowed [`ReplayEvent`] directly and skip the per-bar `InstrumentId` clone
    /// and `Event` allocation. Only invoked when [`Strategy::uses_tick_replay`] is `true`.
    fn on_replay_event(
        &mut self,
        ctx: &StrategyContext<'_>,
        event: &ReplayEvent<'_>,
        out: &mut Vec<OrderIntent>,
    ) {
        let owned = event.to_event();
        self.on_event(ctx, &owned, out);
    }

    /// When true, replay walks [`crate::BarSeries`] by index instead of allocating per-bar events.
    fn uses_tick_replay(&self) -> bool {
        false
    }

    /// Append controls produced by the most recent strategy callback.
    fn drain_controls(&mut self, _out: &mut Vec<StrategyControl>) {}

    /// Final callback after replay, allowing cancellation or deterministic flattening.
    fn on_finish(&mut self, _ctx: &StrategyContext<'_>, _out: &mut Vec<OrderIntent>) {}

    /// Final structured research diagnostics collected during the run.
    fn diagnostics(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }
}

/// No-op for benchmarks.
#[derive(Default)]
pub struct NoopStrategy;

impl Strategy for NoopStrategy {
    fn on_event(&mut self, _: &StrategyContext<'_>, _: &Event, _: &mut Vec<OrderIntent>) {}

    fn on_replay_event(
        &mut self,
        _: &StrategyContext<'_>,
        _: &ReplayEvent<'_>,
        _: &mut Vec<OrderIntent>,
    ) {
    }

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

    fn on_replay_event(
        &mut self,
        ctx: &StrategyContext<'_>,
        event: &ReplayEvent<'_>,
        out: &mut Vec<OrderIntent>,
    ) {
        for child in &mut self.children {
            child.on_replay_event(ctx, event, out);
        }
    }

    fn drain_controls(&mut self, out: &mut Vec<StrategyControl>) {
        for child in &mut self.children {
            child.drain_controls(out);
        }
    }

    fn on_finish(&mut self, ctx: &StrategyContext<'_>, out: &mut Vec<OrderIntent>) {
        for child in &mut self.children {
            child.on_finish(ctx, out);
        }
    }
}
