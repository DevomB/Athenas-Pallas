//! Normalized events fed into the engine.

mod metadata;

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
        #[serde(with = "rfc3339_compat")]
        ts: OffsetDateTime,
        /// Price.
        price: Decimal,
        /// Base quantity.
        qty: Decimal,
        /// Vendor/source identity and ordering fields when available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provenance: Option<MarketDataProvenance>,
    },
    /// Best bid/ask.
    BookL1 {
        /// Instrument.
        instrument: InstrumentId,
        /// When.
        #[serde(with = "rfc3339_compat")]
        ts: OffsetDateTime,
        /// Best bid.
        bid: Decimal,
        /// Best ask.
        ask: Decimal,
        /// Vendor/source identity and ordering fields when available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provenance: Option<MarketDataProvenance>,
    },
    /// Shallow L2 snapshot (bounded depth; venue-specific limit).
    BookL2Snapshot(BookL2Snapshot),
    /// Venue trading/quoting state.
    Status(MarketStatusEvent),
    /// Venue auction imbalance state.
    AuctionImbalance(AuctionImbalanceEvent),
    /// Official exchange statistic such as settlement or open interest.
    Statistic(MarketStatisticEvent),
    /// OHLCV bar.
    Bar {
        /// Instrument.
        instrument: InstrumentId,
        /// When.
        #[serde(with = "rfc3339_compat")]
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
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Bid levels, best first.
    pub bids: Vec<(Decimal, Decimal)>,
    /// Ask levels, best first.
    pub asks: Vec<(Decimal, Decimal)>,
    /// Vendor/source identity and ordering fields when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<MarketDataProvenance>,
}

/// Optional source timestamps and sequence identity retained from a market-data feed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketDataProvenance {
    /// Vendor dataset code.
    pub dataset: String,
    /// Vendor publisher/venue id.
    pub publisher_id: u16,
    /// Vendor numeric instrument id.
    pub instrument_id: u32,
    /// Capture/receive timestamp.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub ts_recv: Option<OffsetDateTime>,
    /// Feed sequence number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
}

/// Venue trading status retained as a first-class replay event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketStatusEvent {
    /// Instrument whose status changed.
    pub instrument: InstrumentId,
    /// Event time.
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Vendor-normalized status action code.
    pub action: u16,
    /// Vendor-normalized reason code.
    pub reason: u16,
    /// Vendor-normalized trading-event code.
    pub trading_event: u16,
    /// Whether trading is active, when supplied.
    pub is_trading: Option<bool>,
    /// Whether quoting is active, when supplied.
    pub is_quoting: Option<bool>,
    /// Whether short sales are restricted, when supplied.
    pub is_short_sell_restricted: Option<bool>,
    /// Feed identity and receive time.
    pub provenance: MarketDataProvenance,
}

/// Auction imbalance fields required by session/auction strategies.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuctionImbalanceEvent {
    /// Instrument in the auction.
    pub instrument: InstrumentId,
    /// Event time.
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Venue reference price.
    pub reference_price: Option<Decimal>,
    /// Indicative clearing/match price.
    pub indicative_match_price: Option<Decimal>,
    /// Quantity paired at the reference price.
    pub paired_qty: Option<u32>,
    /// Unpaired imbalance quantity.
    pub total_imbalance_qty: Option<u32>,
    /// Imbalance side code.
    pub side: Option<String>,
    /// Venue auction type code.
    pub auction_type: Option<String>,
    /// Venue auction status code.
    pub auction_status: u8,
    /// Feed identity and receive time.
    pub provenance: MarketDataProvenance,
}

/// Official venue statistic kept separate from trade-built bars.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketStatisticEvent {
    /// Instrument the statistic describes.
    pub instrument: InstrumentId,
    /// Feed event time.
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Reference time of the statistic.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub ts_ref: Option<OffsetDateTime>,
    /// Vendor-normalized statistic type code.
    pub stat_type: u16,
    /// Price value for price statistics.
    pub price: Option<Decimal>,
    /// Quantity value for non-price statistics.
    pub quantity: Option<i64>,
    /// Add/delete update action.
    pub update_action: u8,
    /// Feed identity and sequence.
    pub provenance: MarketDataProvenance,
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
        /// Client-supplied correlation id.
        #[serde(default)]
        client_order_id: Option<ClientOrderId>,
        /// One-cancels-other group shared by sibling orders.
        #[serde(default)]
        oco_group: Option<String>,
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
        /// Client-supplied correlation id.
        #[serde(default)]
        client_order_id: Option<ClientOrderId>,
        /// One-cancels-other group shared by sibling orders.
        #[serde(default)]
        oco_group: Option<String>,
        /// Sub-strategy attribution when known (paper/sim propagate from working order).
        #[serde(default)]
        strategy_id: Option<StrategyId>,
        /// Execution simulation surface (`bar`, `bbo`, `l2`, or `last`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        simulation_model: Option<String>,
    },
    /// Risk or execution rejection retained for strategy feedback and reporting.
    Rejection(RejectionRecord),
}

/// Stage at which an order request was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionKind {
    /// Rejected before execution by a risk rule.
    Risk,
    /// Rejected by the execution simulator or venue adapter.
    Execution,
}

/// Structured rejected-order record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RejectionRecord {
    /// Replay time at which the rejection occurred.
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Risk or execution stage.
    pub kind: RejectionKind,
    /// Target instrument.
    pub instrument: InstrumentId,
    /// Client correlation id, if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    /// Human-readable rejection reason.
    pub reason: String,
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
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Caller-defined schedule identifier.
    pub id: u32,
}

/// One executed fill for backtest reports.
#[derive(Clone, Debug, Serialize)]
pub struct FillRecord {
    /// When the fill occurred.
    #[serde(with = "rfc3339_compat")]
    pub ts: OffsetDateTime,
    /// Engine order id.
    pub order_id: OrderId,
    /// Filled instrument.
    pub instrument: InstrumentId,
    /// Buy or sell.
    pub side: Side,
    /// Base quantity.
    pub qty: String,
    /// Fill price.
    pub price: String,
    /// Fee paid.
    pub fee: String,
    /// Quote-currency value of one price point per unit, when contract metadata defines one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_multiplier: Option<String>,
    /// Execution simulation surface used for this fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation_model: Option<String>,
    /// Client correlation id, if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    /// OCO group, if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oco_group: Option<String>,
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
    /// One-cancels-other group shared by sibling orders.
    #[serde(default)]
    pub oco_group: Option<String>,
    /// Source for risk rules (e.g. flatten bypasses [`crate::risk::PauseCheck`]).
    #[serde(default)]
    pub source: OrderIntentSource,
    /// Optional sub-strategy label for attributed position tracking (see [`crate::state::GlobalState::strategy_position_qty`] and [`crate::metrics::strategy_position_report`]).
    #[serde(default)]
    pub strategy_id: Option<StrategyId>,
}

mod rfc3339_compat {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};
    use time::OffsetDateTime;

    pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        time::serde::rfc3339::serialize(value, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(text) => {
                OffsetDateTime::parse(&text, &time::format_description::well_known::Rfc3339)
                    .map_err(D::Error::custom)
            }
            legacy => OffsetDateTime::deserialize(legacy).map_err(D::Error::custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn market_event_timestamps_are_rfc3339_strings() {
        let event = Event::Market(MarketEvent::Bar {
            instrument: InstrumentId::new("test", "ES"),
            ts: datetime!(2025-01-02 14:30:00 UTC),
            open: Decimal::ONE,
            high: Decimal::ONE,
            low: Decimal::ONE,
            close: Decimal::ONE,
            volume: Decimal::ONE,
        });

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json["Market"]["Bar"]["ts"],
            serde_json::Value::String("2025-01-02T14:30:00Z".into())
        );
        assert_eq!(
            serde_json::from_value::<Event>(json).unwrap().timestamp(),
            Some(datetime!(2025-01-02 14:30:00 UTC))
        );
    }

    #[test]
    fn market_events_accept_legacy_timestamp_arrays() {
        let json = r#"{"Market":{"Bar":{"instrument":{"exchange":"test","symbol":"ES"},"ts":[2025,2,14,30,0,0,0,0,0],"open":"1","high":"1","low":"1","close":"1","volume":"1"}}}"#;
        assert_eq!(
            serde_json::from_str::<Event>(json).unwrap().timestamp(),
            Some(datetime!(2025-01-02 14:30:00 UTC))
        );
    }
}
