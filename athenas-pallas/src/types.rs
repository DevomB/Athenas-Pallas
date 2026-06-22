//! Core domain types.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

/// Whether the strategy may submit new orders (market/account/control still run).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingState {
    /// Algorithmic order flow from [`crate::strategy::Strategy`] is allowed (subject to risk).
    #[default]
    Enabled,
    /// Strategy hook is skipped; market and account updates still apply (barter-style “trading off”).
    Disabled,
}

/// Asset code for balances (e.g. USDT, BTC).
pub use crate::instrument::Asset;

/// Exchange / symbol newtypes (config & data crates).
pub use crate::instrument::{ExchangeId, Symbol};

/// Instrument = exchange + symbol (engine hot path).
pub use crate::instrument::LegacyInstrumentId as InstrumentId;

/// Order side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// Buy base asset.
    Buy,
    /// Sell base asset.
    Sell,
}

impl Side {
    /// Opposite side.
    pub fn opposite(self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

/// Limit vs market vs stop variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Limit order.
    Limit,
    /// Market order.
    Market,
    /// Stop market - triggers at stop price, then fills at market.
    StopMarket,
    /// Stop limit - triggers at stop price, then rests as limit.
    StopLimit,
}

/// Stable order id (paper/sim).
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderId(pub Uuid);

impl OrderId {
    /// Random id.
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Deterministic id from a venue numeric order id string.
    pub fn from_venue_u64(id: u64) -> Self {
        Self(Uuid::from_u128(id as u128))
    }
}

/// Logical sub-strategy / sleeve id for **per-strategy position attribution** (optional on each order).
///
/// When [`OrderIntent::strategy_id`](crate::events::OrderIntent) is `None`, fills update aggregate
/// [`crate::state::GlobalState::positions`] only. When `Some`, fills also update
/// [`crate::state::GlobalState::strategy_positions`] (see [`GlobalState::strategy_position_qty`](crate::state::GlobalState::strategy_position_qty) and [`GlobalState::position_qty_for_strategy`](crate::state::GlobalState::position_qty_for_strategy)).
/// Bindings (e.g. Python) can expose this as a `strategy_id=` keyword on order requests.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StrategyId(pub String);

impl StrategyId {
    /// New id from a string label (e.g. `"momentum"`).
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for StrategyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Client-supplied correlation id.
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientOrderId(pub String);

/// Order status in local state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Accepted, resting or pending.
    Open,
    /// Fully filled.
    Filled,
    /// User/system canceled.
    Canceled,
    /// Rejected by venue or simulator.
    Rejected,
}

/// Working order snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenOrder {
    /// Id.
    pub id: OrderId,
    /// Instrument.
    pub instrument: InstrumentId,
    /// Side.
    pub side: Side,
    /// Type.
    pub order_type: OrderType,
    /// Limit price (required for limit / stop-limit resting price).
    pub price: Option<Decimal>,
    /// Stop trigger price (required for stop market / stop limit).
    #[serde(default)]
    pub stop_price: Option<Decimal>,
    /// Remaining base quantity.
    pub remaining_qty: Decimal,
    /// Original quantity.
    pub original_qty: Decimal,
    /// Status.
    pub status: OrderStatus,
    /// Sub-strategy attribution (if any); copied from the originating [`crate::events::OrderIntent`].
    #[serde(default)]
    pub strategy_id: Option<StrategyId>,
}

/// Point on equity curve for metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EquityPoint {
    /// Timestamp.
    pub ts: OffsetDateTime,
    /// Mark-to-market equity in quote (e.g. USDT).
    pub equity_quote: Decimal,
}
