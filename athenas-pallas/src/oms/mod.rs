//! Standalone working-order view (order manager surface without the full engine).
//!
//! The live engine still owns [`crate::state::GlobalState::open_orders`], which dereferences like a
//! `HashMap` via [`OrderStore`] for drop-in compatibility. Per-instrument **price-indexed** books
//! (`BTreeMap` keyed by limit/stop price) let the paper gateway visit only the resting orders whose
//! trigger/cross condition could be satisfied by the current L1 or bar high/low — `O(log m + k)`
//! instead of scanning every open order on the instrument each market event.

use crate::types::{InstrumentId, OpenOrder, OrderId, OrderStatus, OrderType, Side};
use rust_decimal::Decimal;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::{BTreeMap, HashMap};
use std::ops::Deref;

type Level = SmallVec<[OrderId; 4]>;

/// Price-sorted resting-order indices for one instrument.
#[derive(Clone, Debug, Default)]
struct InstrumentBooks {
    /// Buy limits keyed by limit price (ascending). A buy limit at `P` crosses when `ask <= P`.
    buy_limits: BTreeMap<Decimal, Level>,
    /// Sell limits keyed by limit price (ascending). A sell limit at `P` crosses when `bid >= P`.
    sell_limits: BTreeMap<Decimal, Level>,
    /// Buy stops keyed by stop price (ascending). A buy stop at `P` triggers when `high >= P`.
    buy_stops: BTreeMap<Decimal, Level>,
    /// Sell stops keyed by stop price (ascending). A sell stop at `P` triggers when `low <= P`.
    sell_stops: BTreeMap<Decimal, Level>,
}

impl InstrumentBooks {
    fn insert(&mut self, o: &OpenOrder) {
        match o.order_type {
            OrderType::Limit => {
                let Some(px) = o.price else { return };
                match o.side {
                    Side::Buy => self.buy_limits.entry(px).or_default().push(o.id.clone()),
                    Side::Sell => self.sell_limits.entry(px).or_default().push(o.id.clone()),
                }
            }
            OrderType::StopMarket | OrderType::StopLimit => {
                let Some(stop) = o.stop_price else { return };
                match o.side {
                    Side::Buy => self.buy_stops.entry(stop).or_default().push(o.id.clone()),
                    Side::Sell => self.sell_stops.entry(stop).or_default().push(o.id.clone()),
                }
            }
            OrderType::Market => {}
        }
    }

    fn remove(&mut self, o: &OpenOrder) {
        let remove_from = |map: &mut BTreeMap<Decimal, Level>, key: Decimal, id: &OrderId| {
            if let Some(level) = map.get_mut(&key) {
                level.retain(|existing| existing != id);
                if level.is_empty() {
                    map.remove(&key);
                }
            }
        };
        match o.order_type {
            OrderType::Limit => {
                if let Some(px) = o.price {
                    match o.side {
                        Side::Buy => remove_from(&mut self.buy_limits, px, &o.id),
                        Side::Sell => remove_from(&mut self.sell_limits, px, &o.id),
                    }
                }
            }
            OrderType::StopMarket | OrderType::StopLimit => {
                if let Some(stop) = o.stop_price {
                    match o.side {
                        Side::Buy => remove_from(&mut self.buy_stops, stop, &o.id),
                        Side::Sell => remove_from(&mut self.sell_stops, stop, &o.id),
                    }
                }
            }
            OrderType::Market => {}
        }
    }

    fn is_empty(&self) -> bool {
        self.buy_limits.is_empty()
            && self.sell_limits.is_empty()
            && self.buy_stops.is_empty()
            && self.sell_stops.is_empty()
    }

    /// Collect order ids whose limit/stop condition could be met by the supplied market snapshot.
    fn pollable_ids(
        &self,
        bid: Option<Decimal>,
        ask: Option<Decimal>,
        high: Option<Decimal>,
        low: Option<Decimal>,
        out: &mut Vec<OrderId>,
    ) {
        if let Some(ask) = ask {
            for (_, ids) in self.buy_limits.range(ask..) {
                out.extend(ids.iter().cloned());
            }
        }
        if let Some(bid) = bid {
            for (_, ids) in self.sell_limits.range(..=bid) {
                out.extend(ids.iter().cloned());
            }
        }
        if let Some(high) = high {
            for (_, ids) in self.buy_stops.range(..=high) {
                out.extend(ids.iter().cloned());
            }
        }
        if let Some(low) = low {
            for (_, ids) in self.sell_stops.range(low..) {
                out.extend(ids.iter().cloned());
            }
        }
    }
}

/// In-memory resting orders keyed by [`OrderId`], with per-instrument price indices.
#[derive(Clone, Debug, Default)]
pub struct OrderStore {
    orders: HashMap<OrderId, OpenOrder>,
    books: FxHashMap<InstrumentId, InstrumentBooks>,
}

impl Deref for OrderStore {
    type Target = HashMap<OrderId, OpenOrder>;

    fn deref(&self) -> &Self::Target {
        &self.orders
    }
}

impl OrderStore {
    /// Empty book.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge a venue-style order update into the book (same rules as
    /// [`crate::state::GlobalState::apply_account`]).
    pub fn apply_order_update(&mut self, o: OpenOrder) {
        if matches!(
            o.status,
            OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Rejected
        ) {
            self.remove_order(&o.id);
        } else {
            self.insert_order(o);
        }
    }

    fn insert_order(&mut self, o: OpenOrder) {
        let id = o.id.clone();
        let instrument = o.instrument.clone();
        if let Some(prev) = self.orders.insert(id.clone(), o) {
            if let Some(book) = self.books.get_mut(&prev.instrument) {
                book.remove(&prev);
                if book.is_empty() {
                    self.books.remove(&prev.instrument);
                }
            }
        }
        if let Some(stored) = self.orders.get(&id) {
            self.books.entry(instrument).or_default().insert(stored);
        }
    }

    fn remove_order(&mut self, id: &OrderId) {
        if let Some(o) = self.orders.remove(id) {
            if let Some(book) = self.books.get_mut(&o.instrument) {
                book.remove(&o);
                if book.is_empty() {
                    self.books.remove(&o.instrument);
                }
            }
        }
    }

    /// Order ids on `instrument` whose resting limit/stop **could** trigger against the snapshot.
    ///
    /// The caller still runs full fill rules (`FillModel`, balance checks) on each candidate; this
    /// only prunes orders that are provably out of range for the current bar/L1.
    pub fn pollable_ids(
        &self,
        instrument: &InstrumentId,
        bid: Option<Decimal>,
        ask: Option<Decimal>,
        high: Option<Decimal>,
        low: Option<Decimal>,
    ) -> Vec<OrderId> {
        let Some(book) = self.books.get(instrument) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        book.pollable_ids(bid, ask, high, low, &mut out);
        out
    }

    /// Iterate the working orders resting on one instrument (clones the matching orders).
    pub fn orders_for_instrument(&self, instrument: &InstrumentId) -> Vec<OpenOrder> {
        let Some(book) = self.books.get(instrument) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        self.collect_all_ids(book, &mut ids);
        ids.into_iter()
            .filter_map(|id| self.orders.get(&id).cloned())
            .collect()
    }

    fn collect_all_ids(&self, book: &InstrumentBooks, out: &mut Vec<OrderId>) {
        out.clear();
        for map in [
            &book.buy_limits,
            &book.sell_limits,
            &book.buy_stops,
            &book.sell_stops,
        ] {
            for ids in map.values() {
                out.extend(ids.iter().cloned());
            }
        }
    }

    /// Number of resting orders on one instrument without materializing them.
    pub fn instrument_order_count(&self, instrument: &InstrumentId) -> usize {
        self.books
            .get(instrument)
            .map(|book| {
                let mut n = 0usize;
                for map in [
                    &book.buy_limits,
                    &book.sell_limits,
                    &book.buy_stops,
                    &book.sell_stops,
                ] {
                    n += map.values().map(|v| v.len()).sum::<usize>();
                }
                n
            })
            .unwrap_or(0)
    }

    /// Clone list of working orders (open / pending).
    pub fn working_orders(&self) -> Vec<OpenOrder> {
        self.orders.values().cloned().collect()
    }

    /// Instruments with at least one resting order in the price index.
    pub fn instruments_with_orders(&self) -> impl Iterator<Item = &InstrumentId> {
        self.books.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InstrumentId, OrderType, Side};

    fn order(
        id: u64,
        inst: &InstrumentId,
        status: OrderStatus,
        order_type: OrderType,
        side: Side,
        price: Option<Decimal>,
        stop: Option<Decimal>,
    ) -> OpenOrder {
        OpenOrder {
            id: OrderId::from_venue_u64(id),
            instrument: inst.clone(),
            side,
            order_type,
            price,
            stop_price: stop,
            remaining_qty: Decimal::ONE,
            original_qty: Decimal::ONE,
            status,
            strategy_id: None,
        }
    }

    #[test]
    fn instrument_index_tracks_add_and_remove() {
        let a = InstrumentId::new("binance", "BTCUSDT");
        let b = InstrumentId::new("binance", "ETHUSDT");
        let mut store = OrderStore::new();

        store.apply_order_update(order(
            1,
            &a,
            OrderStatus::Open,
            OrderType::Limit,
            Side::Buy,
            Some(Decimal::new(100, 0)),
            None,
        ));
        store.apply_order_update(order(
            2,
            &a,
            OrderStatus::Open,
            OrderType::Limit,
            Side::Buy,
            Some(Decimal::new(101, 0)),
            None,
        ));
        store.apply_order_update(order(
            3,
            &b,
            OrderStatus::Open,
            OrderType::Limit,
            Side::Sell,
            Some(Decimal::new(50, 0)),
            None,
        ));

        assert_eq!(store.instrument_order_count(&a), 2);
        assert_eq!(store.instrument_order_count(&b), 1);

        store.apply_order_update(order(
            1,
            &a,
            OrderStatus::Filled,
            OrderType::Limit,
            Side::Buy,
            Some(Decimal::new(100, 0)),
            None,
        ));
        assert_eq!(store.instrument_order_count(&a), 1);
    }

    #[test]
    fn price_index_returns_only_crossing_buy_limits() {
        let a = InstrumentId::new("binance", "BTCUSDT");
        let mut store = OrderStore::new();
        store.apply_order_update(order(
            1,
            &a,
            OrderStatus::Open,
            OrderType::Limit,
            Side::Buy,
            Some(Decimal::new(99, 0)),
            None,
        ));
        store.apply_order_update(order(
            2,
            &a,
            OrderStatus::Open,
            OrderType::Limit,
            Side::Buy,
            Some(Decimal::new(101, 0)),
            None,
        ));
        store.apply_order_update(order(
            3,
            &a,
            OrderStatus::Open,
            OrderType::Limit,
            Side::Buy,
            Some(Decimal::new(105, 0)),
            None,
        ));

        let ask = Decimal::new(100, 0);
        let ids = store.pollable_ids(&a, None, Some(ask), None, None);
        // Buy limits at 101 and 105 cross when ask=100; 99 does not.
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&OrderId::from_venue_u64(2)));
        assert!(ids.contains(&OrderId::from_venue_u64(3)));
        assert!(!ids.contains(&OrderId::from_venue_u64(1)));
    }

    #[test]
    fn stop_index_respects_bar_high_low() {
        let a = InstrumentId::new("binance", "BTCUSDT");
        let mut store = OrderStore::new();
        store.apply_order_update(order(
            1,
            &a,
            OrderStatus::Open,
            OrderType::StopMarket,
            Side::Buy,
            None,
            Some(Decimal::new(102, 0)),
        ));
        store.apply_order_update(order(
            2,
            &a,
            OrderStatus::Open,
            OrderType::StopMarket,
            Side::Sell,
            None,
            Some(Decimal::new(98, 0)),
        ));

        let high = Decimal::new(103, 0);
        let low = Decimal::new(97, 0);
        let ids = store.pollable_ids(&a, None, None, Some(high), Some(low));
        assert_eq!(ids.len(), 2);
    }
}
