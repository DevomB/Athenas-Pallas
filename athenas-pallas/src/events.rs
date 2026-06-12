//! Normalized events fed into the engine.

use crate::types::{
    Asset, ClientOrderId, InstrumentId, OrderId, OrderStatus, OrderType, Side, StrategyId,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Public market update.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MarketEvent {
    /// Last trade (price, qty).
    Trade {
        /// Instrument.
        instrument: InstrumentId,
        /// When.
        ts: OffsetDateTime,
        /// Price.
        price: Decimal,
        /// Base quantity.
        qty: Decimal,
    },
    /// Best bid/ask.
    BookL1 {
        /// Instrument.
        instrument: InstrumentId,
        /// When.
        ts: OffsetDateTime,
        /// Best bid.
        bid: Decimal,
        /// Best ask.
        ask: Decimal,
    },
    /// Shallow L2 snapshot (bounded depth; venue-specific limit).
    BookL2Snapshot(BookL2Snapshot),
    /// OHLCV bar.
    Bar {
        /// Instrument.
        instrument: InstrumentId,
        /// When.
        ts: OffsetDateTime,
        /// Open.
        open: Decimal,
        /// High.
        high: Decimal,
        /// Low.
        low: Decimal,
        /// Close.
        close: Decimal,
        /// Volume.
        volume: Decimal,
    },
}

/// Top-of-book depth snapshot (price, qty) levels.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookL2Snapshot {
    /// Instrument.
    pub instrument: InstrumentId,
    /// When.
    pub ts: OffsetDateTime,
    /// Bid levels, best first.
    pub bids: Vec<(Decimal, Decimal)>,
    /// Ask levels, best first.
    pub asks: Vec<(Decimal, Decimal)>,
}

/// Private / account-side update.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AccountEvent {
    /// Balance for asset.
    Balance {
        /// Asset.
        asset: Asset,
        /// Free balance.
        free: Decimal,
    },
    /// Order update.
    OrderUpdate {
        /// Id.
        id: OrderId,
        /// Instrument.
        instrument: InstrumentId,
        /// Side.
        side: Side,
        /// Type.
        order_type: OrderType,
        /// Limit price.
        price: Option<Decimal>,
        /// Remaining qty.
        remaining_qty: Decimal,
        /// Original qty.
        original_qty: Decimal,
        /// Status.
        status: OrderStatus,
        /// Sub-strategy attribution (if any).
        #[serde(default)]
        strategy_id: Option<StrategyId>,
    },
    /// Fill notice (optional detail; state can also infer from OrderUpdate).
    Fill {
        /// Order id.
        order_id: OrderId,
        /// Instrument.
        instrument: InstrumentId,
        /// Side (taker side).
        side: Side,
        /// Fill price.
        price: Decimal,
        /// Filled base qty.
        qty: Decimal,
        /// Fee in fee asset (positive number).
        fee: Decimal,
        /// Fee asset.
        fee_asset: Asset,
        /// Sub-strategy attribution when known (paper/sim propagate from working order).
        #[serde(default)]
        strategy_id: Option<StrategyId>,
    },
}

/// Control-plane commands (also used for in-process control).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ControlEvent {
    /// Pause new orders; engine keeps processing cancels/market data.
    Pause,
    /// Resume.
    Resume,
    /// Cancel all open orders.
    CancelAll,
    /// Flatten: cancel all then market-close net base (optional extension point).
    Flatten,
    /// Turn off algorithmic trading (strategy not invoked); data and control actions still run.
    DisableTrading,
    /// Re-enable algorithmic trading after [`ControlEvent::DisableTrading`].
    EnableTrading,
}

/// Timer tick for periodic strategy logic.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimerEvent {
    /// Wall clock (UTC recommended).
    pub ts: OffsetDateTime,
    /// Schedule identifier from [`crate::engine::TimerSchedule`].
    pub id: u32,
}

/// One executed fill for backtest reports.
#[derive(Clone, Debug, Serialize)]
pub struct FillRecord {
    /// When the fill occurred.
    pub ts: OffsetDateTime,
    /// Buy or sell.
    pub side: Side,
    /// Base quantity.
    pub qty: String,
    /// Fill price.
    pub price: String,
    /// Fee paid.
    pub fee: String,
}

/// Unified engine input.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    /// Market data.
    Market(MarketEvent),
    /// Account / execution feedback.
    Account(AccountEvent),
    /// Control.
    Control(ControlEvent),
    /// Timer.
    Timer(TimerEvent),
}

impl Event {
    /// Extract instrument from market events if present.
    pub fn instrument(&self) -> Option<&InstrumentId> {
        match self {
            Event::Market(MarketEvent::Trade { instrument, .. }) => Some(instrument),
            Event::Market(MarketEvent::BookL1 { instrument, .. }) => Some(instrument),
            Event::Market(MarketEvent::BookL2Snapshot(s)) => Some(&s.instrument),
            Event::Market(MarketEvent::Bar { instrument, .. }) => Some(instrument),
            Event::Account(AccountEvent::OrderUpdate { instrument, .. }) => Some(instrument),
            Event::Account(AccountEvent::Fill { instrument, .. }) => Some(instrument),
            _ => None,
        }
    }
}

/// Origin of an [`OrderIntent`] (risk / pause semantics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderIntentSource {
    /// Strategy or operator-initiated.
    #[default]
    User,
    /// Emergency flatten path after [`ControlEvent::Flatten`] (may bypass pause).
    Flatten,
}

/// Strategy order request (before risk/execution).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderIntent {
    /// Target instrument.
    pub instrument: InstrumentId,
    /// Side.
    pub side: Side,
    /// Limit or market.
    pub order_type: OrderType,
    /// Limit price (required for limit).
    pub price: Option<Decimal>,
    /// Base quantity.
    pub qty: Decimal,
    /// Optional client id.
    pub client_order_id: Option<ClientOrderId>,
    /// Source for risk rules (e.g. flatten bypasses [`crate::risk::PauseCheck`]).
    #[serde(default)]
    pub source: OrderIntentSource,
    /// Optional sub-strategy label for attributed position tracking (see [`crate::state::GlobalState::strategy_position_qty`] and [`crate::metrics::strategy_position_report`]).
    #[serde(default)]
    pub strategy_id: Option<StrategyId>,
}
