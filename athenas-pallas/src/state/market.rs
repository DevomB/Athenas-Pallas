use super::GlobalState;
use crate::bar::{ticks_to_decimal, Bar};
use crate::events::MarketEvent;
use crate::types::InstrumentId;
use rust_decimal::Decimal;
use time::OffsetDateTime;

impl GlobalState {
    fn synthetic_bid_ask(mid: Decimal, half_spread_bps: Decimal) -> (Decimal, Decimal) {
        let half_spread = mid * half_spread_bps / Decimal::from(10_000u64);
        (mid - half_spread, mid + half_spread)
    }

    fn clear_completed_bar_range(&mut self, ix: usize) {
        self.bar_high[ix] = None;
        self.bar_low[ix] = None;
    }

    fn apply_open_market(
        &mut self,
        ix: usize,
        ts: OffsetDateTime,
        open: Decimal,
        half_spread_bps: Decimal,
    ) {
        let (bid, ask) = Self::synthetic_bid_ask(open, half_spread_bps);
        self.last_trade[ix] = Some((ts, open));
        self.l1[ix] = Some((ts, bid, ask));
        self.clear_completed_bar_range(ix);
        self.last_event_ts = Some(ts);
    }

    fn apply_trade_price(&mut self, ix: usize, ts: OffsetDateTime, price: Decimal) {
        self.last_trade[ix] = Some((ts, price));
        self.clear_completed_bar_range(ix);
        self.last_event_ts = Some(ts);
    }

    fn apply_quote(&mut self, ix: usize, ts: OffsetDateTime, bid: Decimal, ask: Decimal) {
        self.l1[ix] = Some((ts, bid, ask));
        self.clear_completed_bar_range(ix);
        self.last_event_ts = Some(ts);
    }

    fn apply_completed_bar(
        &mut self,
        ix: usize,
        ts: OffsetDateTime,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        half_spread_bps: Decimal,
    ) {
        let (bid, ask) = Self::synthetic_bid_ask(close, half_spread_bps);
        self.last_trade[ix] = Some((ts, close));
        self.bar_close[ix] = Some(close);
        self.bar_high[ix] = Some(high);
        self.bar_low[ix] = Some(low);
        self.l1[ix] = Some((ts, bid, ask));
        self.last_event_ts = Some(ts);
    }

    /// Set a preloaded bar's opening market, used to execute prior-bar orders without lookahead.
    pub fn apply_bar_open(
        &mut self,
        ix: usize,
        bar: &Bar,
        tick_size: Decimal,
        half_spread_bps: Decimal,
    ) {
        let Some(ts) = bar.timestamp() else {
            return;
        };
        self.apply_open_market(
            ix,
            ts,
            ticks_to_decimal(bar.open_ticks, tick_size),
            half_spread_bps,
        );
    }

    /// Set an owned bar event's opening market before applying its completed OHLC values.
    pub fn apply_bar_event_open(
        &mut self,
        instrument: &InstrumentId,
        ts: OffsetDateTime,
        open: Decimal,
    ) {
        let Some(ix) = self.registry.index_of(instrument).map(|index| index.0) else {
            return;
        };
        self.apply_open_market(ix, ts, open, self.synthetic_half_spread_bps);
    }

    /// Update market state from a completed preloaded bar (tick-native replay).
    pub fn apply_bar(
        &mut self,
        ix: usize,
        bar: &Bar,
        tick_size: Decimal,
        half_spread_bps: Decimal,
    ) {
        let Some(ts) = bar.timestamp() else {
            return;
        };
        self.apply_completed_bar(
            ix,
            ts,
            ticks_to_decimal(bar.high_ticks, tick_size),
            ticks_to_decimal(bar.low_ticks, tick_size),
            ticks_to_decimal(bar.close_ticks, tick_size),
            half_spread_bps,
        );
    }

    /// Apply a market event (read-only book/trade updates).
    pub fn apply_market(&mut self, event: &MarketEvent) {
        match event {
            MarketEvent::Trade {
                instrument,
                ts,
                price,
                ..
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|index| index.0) {
                    self.apply_trade_price(ix, *ts, *price);
                }
            }
            MarketEvent::BookL1 {
                instrument,
                ts,
                bid,
                ask,
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|index| index.0) {
                    self.apply_quote(ix, *ts, *bid, *ask);
                }
            }
            MarketEvent::BookL2Snapshot(snapshot) => {
                if let Some(ix) = self
                    .registry
                    .index_of(&snapshot.instrument)
                    .map(|index| index.0)
                {
                    self.l2[ix] = Some(snapshot.clone());
                }
            }
            MarketEvent::Bar {
                instrument,
                ts,
                high,
                low,
                close,
                ..
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|index| index.0) {
                    self.apply_completed_bar(
                        ix,
                        *ts,
                        *high,
                        *low,
                        *close,
                        self.synthetic_half_spread_bps,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::{InstrumentMeta, InstrumentRegistry};
    use crate::types::Asset;
    use std::collections::HashMap;

    fn state(instrument: &InstrumentId) -> GlobalState {
        let instruments = HashMap::from([(
            instrument.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        )]);
        GlobalState::new(
            InstrumentRegistry::from_instruments(instruments),
            HashMap::new(),
        )
    }

    #[test]
    fn apply_bar_uses_configured_synthetic_spread() {
        let instrument = InstrumentId::new("test", "BTCUSDT");
        let mut state = state(&instrument);
        let tick = crate::bar::default_tick_size();
        let bar = Bar {
            ts_unix_nanos: OffsetDateTime::UNIX_EPOCH.unix_timestamp_nanos() as i64,
            open_ticks: 9_900_000_000,
            high_ticks: 10_200_000_000,
            low_ticks: 9_800_000_000,
            close_ticks: 10_000_000_000,
            volume_lots: 1,
        };

        state.apply_bar(0, &bar, tick, Decimal::from(100u64));

        let (_, bid, ask) = state.l1[0].unwrap();
        assert_eq!(bid, Decimal::new(99, 0));
        assert_eq!(ask, Decimal::new(101, 0));
    }

    #[test]
    fn trade_after_bar_clears_completed_range() {
        let instrument = InstrumentId::new("test", "BTCUSDT");
        let mut state = state(&instrument);
        let ts = OffsetDateTime::UNIX_EPOCH;
        state.apply_market(&MarketEvent::Bar {
            instrument: instrument.clone(),
            ts,
            open: Decimal::from(99),
            high: Decimal::from(102),
            low: Decimal::from(98),
            close: Decimal::from(100),
            volume: Decimal::ONE,
        });

        state.apply_market(&MarketEvent::Trade {
            instrument,
            ts: ts + time::Duration::seconds(1),
            price: Decimal::from(101),
            qty: Decimal::ONE,
        });

        assert_eq!(
            state.last_trade[0].map(|(_, price)| price),
            Some(Decimal::from(101))
        );
        assert_eq!(state.bar_high[0], None);
        assert_eq!(state.bar_low[0], None);
    }
}
