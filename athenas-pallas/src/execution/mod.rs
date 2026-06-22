//! Sync execution backends for backtest replay.

mod fills;
mod sim;
mod sync_paper;

pub use fills::{FillEngine, PaperConfig};
pub use sim::SimGateway;
pub use sync_paper::{SyncExecutionGateway, SyncPaperGateway};

use crate::types::Side;
use rust_decimal::Decimal;

/// Inline buffer for synchronous gateway results.
pub type AccountEvents = smallvec::SmallVec<[crate::events::AccountEvent; 4]>;

/// Apply market-order slippage in basis points around `mid`.
pub(crate) fn apply_slippage(side: Side, mid: Decimal, bps: Decimal) -> Decimal {
    let adj = mid * bps / Decimal::from(10_000u64);
    match side {
        Side::Buy => mid + adj,
        Side::Sell => mid - adj,
    }
}
