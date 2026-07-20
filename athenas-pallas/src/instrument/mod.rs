//! Instrument identifiers, metadata, and dense indexing.

mod asset;
mod index;
pub mod pricing;
mod registry;
pub mod ticks;

pub use asset::{Asset, ExchangeId, Symbol};
pub use index::InstrumentIndex;
pub use pricing::{
    apply_perp_funding, bond_coupon_cash, margin_required, option_intrinsic_value,
    should_exercise_european, OptionKind,
};
pub use registry::{AssetClass, InstrumentId, InstrumentMeta, InstrumentRegistry};
pub use ticks::{notional_decimal, notional_steps, PriceTicks, QtyLots};
