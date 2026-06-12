//! Instrument kinds and identifiers.

use crate::instrument::asset::{Asset, ExchangeId, Symbol};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Underlying base/quote pair.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Underlying {
    /// Base asset.
    pub base: Asset,
    /// Quote asset.
    pub quote: Asset,
}

/// Option call vs put.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionKind {
    /// Call.
    Call,
    /// Put.
    Put,
}

/// Option exercise style.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionExercise {
    /// American.
    American,
    /// European.
    European,
}

/// Future contract metadata.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FutureContract {
    /// Expiry (exchange string).
    pub expiry: String,
}

/// Option contract metadata.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OptionContract {
    /// Strike price string.
    pub strike: String,
    /// Call or put.
    pub kind: OptionKind,
    /// Exercise style.
    pub exercise: OptionExercise,
    /// Expiry.
    pub expiry: String,
}

/// Instrument product kind (barter-compatible JSON).
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstrumentKind {
    /// Spot.
    Spot,
    /// Perpetual swap.
    Perpetual,
    /// Dated future.
    Future {
        /// Contract details.
        #[serde(flatten)]
        contract: FutureContract,
    },
    /// Option.
    Option {
        /// Contract details.
        #[serde(flatten)]
        contract: OptionContract,
    },
}

/// Full instrument identity: exchange + venue symbol + kind.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InstrumentId {
    /// Venue.
    pub exchange: ExchangeId,
    /// Symbol on venue (e.g. BTCUSDT).
    pub name_exchange: Symbol,
    /// Underlying assets.
    pub underlying: Underlying,
    /// Kind (spot, perp, etc.).
    pub kind: InstrumentKind,
}

impl InstrumentId {
    /// Build from config row fields.
    pub fn from_config(
        exchange: ExchangeId,
        name_exchange: Symbol,
        underlying: Underlying,
        kind: InstrumentKind,
    ) -> Self {
        Self {
            exchange,
            name_exchange,
            underlying,
            kind,
        }
    }

    /// Legacy-style id: `exchange:symbol` string key for maps.
    pub fn key(&self) -> String {
        format!("{}:{}", self.exchange, self.name_exchange)
    }
}

impl fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key())
    }
}
