//! Sync execution backends for backtest replay.

mod fills;
mod sync_paper;

pub use fills::{FillEngine, PaperConfig};
pub use sync_paper::{PaperExecution, SyncExecutionGateway};

use crate::types::Side;
use rust_decimal::Decimal;

/// Inline buffer for synchronous gateway results.
pub type AccountEvents = smallvec::SmallVec<[crate::events::AccountEvent; 4]>;

/// Optional crossing rule for resting limit orders.
pub trait FillModel: Send + Sync {
    /// Stable label used in diagnostics.
    fn name(&self) -> &'static str;

    /// Return whether a resting limit crosses the current touch.
    fn limit_would_fill(&self, side: Side, limit: Decimal, bid: Decimal, ask: Decimal) -> bool;
}

/// Fill limits when they cross the current touch.
#[derive(Default)]
pub struct TouchCrossFillModel;

impl FillModel for TouchCrossFillModel {
    fn name(&self) -> &'static str {
        "touch_cross"
    }

    fn limit_would_fill(&self, side: Side, limit: Decimal, bid: Decimal, ask: Decimal) -> bool {
        match side {
            Side::Buy => limit >= ask,
            Side::Sell => limit <= bid,
        }
    }
}

/// Apply market-order slippage in basis points around `mid`.
pub(crate) fn apply_slippage(side: Side, mid: Decimal, bps: Decimal) -> Decimal {
    let adj = mid * bps / Decimal::from(10_000u64);
    match side {
        Side::Buy => mid + adj,
        Side::Sell => mid - adj,
    }
}
