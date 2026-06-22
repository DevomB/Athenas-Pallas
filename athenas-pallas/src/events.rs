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
    /// Incremental balance change (venue user stream).
    BalanceDelta {
        /// Asset.
        asset: Asset,
        /// Signed delta applied to free balance.
        delta: Decimal,
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
        /// Stop trigger price when applicable.
        #[serde(default)]
        stop_price: Option<Decimal>,
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
    /// Sub-strategy attribution when the originating order carried a [`StrategyId`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_id: Option<StrategyId>,
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

    /// Timestamp carried by market and timer events.
    ///
    /// Returns `None` for account and control events, which have no intrinsic event time.
    /// Replay paths use this to avoid accidental wall-clock (`now_utc`) reads.
    pub fn timestamp(&self) -> Option<OffsetDateTime> {
        match self {
            Event::Market(MarketEvent::Trade { ts, .. }) => Some(*ts),
            Event::Market(MarketEvent::BookL1 { ts, .. }) => Some(*ts),
            Event::Market(MarketEvent::BookL2Snapshot(s)) => Some(s.ts),
            Event::Market(MarketEvent::Bar { ts, .. }) => Some(*ts),
            Event::Timer(t) => Some(t.ts),
            _ => None,
        }
    }

    /// [`Event::timestamp`], falling back to wall-clock `now` for events without an intrinsic time.
    ///
    /// Prefer [`Event::timestamp`] in deterministic replay paths; use this only where a concrete
    /// timestamp is required for live/async ingestion.
    pub fn timestamp_or_now(&self) -> OffsetDateTime {
        self.timestamp().unwrap_or_else(OffsetDateTime::now_utc)
    }

    /// [`Event::timestamp`] as Unix nanoseconds (for compact audit records).
    pub fn timestamp_unix_nanos(&self) -> Option<i128> {
        self.timestamp().map(|ts| ts.unix_timestamp_nanos())
    }
}

/// Borrowed market event for the zero-allocation tick-replay fast path.
///
/// Unlike [`Event`], the bar variant borrows its [`InstrumentId`] from the data source rather than
/// cloning it per bar (and skips the owning `Event`/`MarketEvent` enum allocation in the hottest
/// loop). Strategies opt in by overriding [`crate::strategy::Strategy::on_replay_event`]; the
/// default implementation materializes an owned [`Event`] via [`ReplayEvent::to_event`] and
/// forwards to `on_event`, so existing strategies keep working unchanged.
#[derive(Clone, Debug)]
pub enum ReplayEvent<'a> {
    /// OHLCV bar with the instrument borrowed from the source.
    Bar {
        /// Instrument (borrowed).
        instrument: &'a InstrumentId,
        /// Bar close time.
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

impl ReplayEvent<'_> {
    /// Event timestamp.
    pub fn timestamp(&self) -> OffsetDateTime {
        match self {
            ReplayEvent::Bar { ts, .. } => *ts,
        }
    }

    /// Borrowed instrument.
    pub fn instrument(&self) -> &InstrumentId {
        match self {
            ReplayEvent::Bar { instrument, .. } => instrument,
        }
    }

    /// Materialize an owned [`Event`] (clones the instrument). Used by the default
    /// [`crate::strategy::Strategy::on_replay_event`] bridge.
    pub fn to_event(&self) -> Event {
        match self {
            ReplayEvent::Bar {
                instrument,
                ts,
                open,
                high,
                low,
                close,
                volume,
            } => Event::Market(MarketEvent::Bar {
                instrument: (*instrument).clone(),
                ts: *ts,
                open: *open,
                high: *high,
                low: *low,
                close: *close,
                volume: *volume,
            }),
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
    /// Limit price (required for limit / stop-limit).
    pub price: Option<Decimal>,
    /// Stop trigger (required for stop market / stop limit).
    #[serde(default)]
    pub stop_price: Option<Decimal>,
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
