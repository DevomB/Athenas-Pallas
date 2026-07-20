//! Instrument identifiers, metadata, and dense indexing.

mod asset;
mod index;
mod kind;
pub mod pricing;
mod registry;
pub mod ticks;

pub use asset::{Asset, ExchangeId, Symbol};
pub use index::InstrumentIndex;
pub use kind::{
    FutureContract, InstrumentId, InstrumentKind, OptionContract, OptionExercise, OptionKind,
    Underlying,
};
pub use pricing::{
    apply_perp_funding, bond_coupon_cash, margin_required, option_intrinsic_value,
    should_exercise_european,
};
pub use registry::{AssetClass, InstrumentMeta, InstrumentRegistry, LegacyInstrumentId};
pub use ticks::{notional_decimal, notional_steps, PriceTicks, QtyLots};
