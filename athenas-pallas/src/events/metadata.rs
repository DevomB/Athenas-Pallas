use super::{AccountEvent, Event, MarketEvent};
use crate::types::InstrumentId;
use time::OffsetDateTime;

impl MarketEvent {
    fn instrument(&self) -> &InstrumentId {
        match self {
            Self::Trade { instrument, .. }
            | Self::BookL1 { instrument, .. }
            | Self::Bar { instrument, .. } => instrument,
            Self::BookL2Snapshot(snapshot) => &snapshot.instrument,
            Self::Status(status) => &status.instrument,
            Self::AuctionImbalance(imbalance) => &imbalance.instrument,
            Self::Statistic(statistic) => &statistic.instrument,
        }
    }

    fn timestamp(&self) -> OffsetDateTime {
        match self {
            Self::Trade { ts, .. } | Self::BookL1 { ts, .. } | Self::Bar { ts, .. } => *ts,
            Self::BookL2Snapshot(snapshot) => snapshot.ts,
            Self::Status(status) => status.ts,
            Self::AuctionImbalance(imbalance) => imbalance.ts,
            Self::Statistic(statistic) => statistic.ts,
        }
    }
}

impl AccountEvent {
    fn instrument(&self) -> Option<&InstrumentId> {
        match self {
            Self::OrderUpdate { instrument, .. } | Self::Fill { instrument, .. } => {
                Some(instrument)
            }
            Self::Rejection(rejection) => Some(&rejection.instrument),
            Self::Balance { .. } | Self::BalanceDelta { .. } => None,
        }
    }
}

impl Event {
    /// Extract instrument from market and instrument-specific account events.
    pub fn instrument(&self) -> Option<&InstrumentId> {
        match self {
            Self::Market(event) => Some(event.instrument()),
            Self::Account(event) => event.instrument(),
            Self::Control(_) | Self::Timer(_) => None,
        }
    }

    /// Timestamp carried by market and timer events.
    ///
    /// Returns `None` for account and control events, which have no intrinsic event time.
    /// Replay paths use this to avoid accidental wall-clock (`now_utc`) reads.
    pub fn timestamp(&self) -> Option<OffsetDateTime> {
        match self {
            Self::Market(event) => Some(event.timestamp()),
            Self::Timer(timer) => Some(timer.ts),
            Self::Account(_) | Self::Control(_) => None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{BookL2Snapshot, ControlEvent, RejectionKind, RejectionRecord, TimerEvent};
    use crate::types::{Asset, InstrumentId};
    use rust_decimal::Decimal;

    #[test]
    fn event_metadata_delegates_by_event_family() {
        let instrument = InstrumentId::new("test", "BTCUSDT");
        let ts = OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(42);
        let market = Event::Market(MarketEvent::BookL2Snapshot(BookL2Snapshot {
            instrument: instrument.clone(),
            ts,
            bids: vec![(Decimal::ONE, Decimal::ONE)],
            asks: vec![],
            provenance: None,
        }));
        let rejection = Event::Account(AccountEvent::Rejection(RejectionRecord {
            ts,
            kind: RejectionKind::Risk,
            instrument: instrument.clone(),
            client_order_id: None,
            reason: "test".into(),
        }));
        let balance = Event::Account(AccountEvent::Balance {
            asset: Asset("USDT".into()),
            free: Decimal::ONE,
        });
        let timer = Event::Timer(TimerEvent { ts, id: 7 });
        let control = Event::Control(ControlEvent::Pause);

        assert_eq!(market.instrument(), Some(&instrument));
        assert_eq!(market.timestamp(), Some(ts));
        assert_eq!(
            market.timestamp_unix_nanos(),
            Some(ts.unix_timestamp_nanos())
        );
        assert_eq!(rejection.instrument(), Some(&instrument));
        assert_eq!(rejection.timestamp(), None);
        assert_eq!(balance.instrument(), None);
        assert_eq!(timer.timestamp(), Some(ts));
        assert_eq!(control.instrument(), None);
    }
}
