//! Authoritative in-memory book, balances, orders, and positions.

pub use crate::instrument::{
    InstrumentFilter, InstrumentIndex, InstrumentMeta, InstrumentRegistry,
};

use crate::events::{AccountEvent, BookL2Snapshot, FillRecord, MarketEvent};
use crate::oms::OrderStore;
use crate::types::{Asset, InstrumentId, OpenOrder, Side, StrategyId};
use rust_decimal::Decimal;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;
use time::{Date, OffsetDateTime};

/// Central view updated serially by the engine.
#[derive(Clone, Debug)]
pub struct GlobalState {
    /// Registered instruments and dense indices.
    pub registry: InstrumentRegistry,
    /// Last trade price per instrument row.
    pub last_trade: Vec<Option<(OffsetDateTime, Decimal)>>,
    /// Best bid/ask per row.
    pub l1: Vec<Option<(OffsetDateTime, Decimal, Decimal)>>,
    /// Open orders by id (OMS surface, see [`crate::oms::OrderStore`]).
    pub open_orders: OrderStore,
    /// Free balances.
    pub balances: HashMap<Asset, Decimal>,
    /// Net base position per instrument row (signed) - venue / account **aggregate**.
    pub positions: Vec<Decimal>,
    /// Attributed net base per `(instrument_row, strategy_id)` when fills carry a [`StrategyId`].
    ///
    /// Sums over strategies may differ from [`Self::positions`] if some fills are untagged or cross-strategy hedges.
    pub strategy_positions: FxHashMap<(usize, StrategyId), Decimal>,
    /// Latest shallow L2 snapshot per row (if subscribed).
    pub l2: Vec<Option<BookL2Snapshot>>,
    /// Mark-to-market equity anchor for UTC calendar daily loss (quote asset must match risk rule).
    pub risk_day_anchor: Option<(Date, Decimal)>,
    /// When set, [`Self::refresh_daily_risk_anchor`] uses this quote for equity day rollover (UTC date).
    pub daily_risk_quote: Option<Asset>,
    /// Trading paused (risk/engine).
    pub paused: bool,
    /// Algorithmic trading enabled/disabled (distinct from operator [`Self::paused`]).
    pub trading_state: crate::types::TradingState,
    /// Last bar close per row (fallback for mid).
    pub bar_close: Vec<Option<Decimal>>,
    /// Last bar high per row (intrabar fill checks).
    pub bar_high: Vec<Option<Decimal>>,
    /// Last bar low per row (intrabar fill checks).
    pub bar_low: Vec<Option<Decimal>>,
    /// Fill count for reporting.
    pub fill_count: u64,
    /// Timestamp of the last market event applied.
    pub last_event_ts: Option<OffsetDateTime>,
    /// Fill blotter for JSON export.
    pub fill_log: Vec<FillRecord>,
    /// Half-spread for synthetic L1 from bar close (basis points).
    pub synthetic_half_spread_bps: Decimal,
}

impl GlobalState {
    /// New state from a registry and optional initial balances.
    pub fn new(registry: InstrumentRegistry, initial_balances: HashMap<Asset, Decimal>) -> Self {
        let n = registry.len();
        Self {
            registry,
            last_trade: vec![None; n],
            l1: vec![None; n],
            open_orders: OrderStore::new(),
            balances: initial_balances,
            positions: vec![Decimal::ZERO; n],
            strategy_positions: FxHashMap::default(),
            l2: vec![None; n],
            risk_day_anchor: None,
            daily_risk_quote: None,
            paused: false,
            trading_state: crate::types::TradingState::Enabled,
            bar_close: vec![None; n],
            bar_high: vec![None; n],
            bar_low: vec![None; n],
            fill_count: 0,
            last_event_ts: None,
            fill_log: Vec::new(),
            synthetic_half_spread_bps: Decimal::from(5u64),
        }
    }

    /// Drain recorded fills (for report generation).
    pub fn take_fill_log(&mut self) -> Vec<FillRecord> {
        std::mem::take(&mut self.fill_log)
    }

    /// Refresh UTC-day equity anchor for [`crate::risk::MaxDailyLossQuote`] after rollover or first tick.
    pub fn refresh_daily_risk_anchor(&mut self, now: OffsetDateTime) {
        let Some(ref quote) = self.daily_risk_quote else {
            return;
        };
        let today = now.date();
        match self.risk_day_anchor {
            None => {
                self.risk_day_anchor = Some((today, self.mark_equity_quote(quote)));
            }
            Some((day, _)) if day != today => {
                self.risk_day_anchor = Some((today, self.mark_equity_quote(quote)));
            }
            _ => {}
        }
    }

    /// Mark-to-market equity in `quote` using mids (open positions valued at [`Self::mid_or_last`]).
    pub fn mark_equity_quote(&self, quote: &crate::types::Asset) -> Decimal {
        self.portfolio_equity_for_quote(quote)
    }

    /// Increment a free balance (coupons, funding, etc.).
    pub fn apply_balance_delta(&mut self, asset: &Asset, delta: Decimal) {
        let entry = self.balances.entry(asset.clone()).or_insert(Decimal::ZERO);
        *entry += delta;
    }

    /// Portfolio mark-to-market in one quote currency (avoids double-counting shared cash).
    pub fn portfolio_equity_for_quote(&self, quote: &Asset) -> Decimal {
        let mut quote_cash_added = false;
        let mut total = Decimal::ZERO;
        for ix in 0..self.registry.len() {
            let Some(meta) = self.registry.meta(InstrumentIndex(ix)) else {
                continue;
            };
            if meta.quote != *quote {
                continue;
            }
            if !quote_cash_added {
                total += self.balances.get(quote).copied().unwrap_or(Decimal::ZERO);
                quote_cash_added = true;
            }
            let mid = self.mid_or_last_ix(ix).unwrap_or(Decimal::ZERO);
            let base = self
                .balances
                .get(&meta.base)
                .copied()
                .unwrap_or(Decimal::ZERO);
            total += Self::position_exposure(meta, base, mid);
        }
        if !quote_cash_added {
            total += self.balances.get(quote).copied().unwrap_or(Decimal::ZERO);
        }
        total
    }

    /// Total portfolio equity across all registered quote currencies.
    pub fn portfolio_equity(&self) -> Decimal {
        let mut quotes = FxHashSet::default();
        for ix in 0..self.registry.len() {
            if let Some(meta) = self.registry.meta(InstrumentIndex(ix)) {
                quotes.insert(meta.quote.clone());
            }
        }
        quotes
            .iter()
            .map(|q| self.portfolio_equity_for_quote(q))
            .fold(Decimal::ZERO, |a, b| a + b)
    }

    fn position_exposure(meta: &InstrumentMeta, base: Decimal, mid: Decimal) -> Decimal {
        match meta.asset_class {
            crate::instrument::AssetClass::Future
            | crate::instrument::AssetClass::Perpetual
            | crate::instrument::AssetClass::Option => {
                let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
                base * mid * mult
            }
            _ => base * mid,
        }
    }

    /// Mid price or last trade fallback.
    pub fn mid_or_last(&self, inst: &InstrumentId) -> Option<Decimal> {
        let ix = self.registry.index_of(inst)?.0;
        if let Some(Some((_, bid, ask))) = self.l1.get(ix) {
            Some((*bid + *ask) / Decimal::from(2u64))
        } else if let Some(Some((_, p))) = self.last_trade.get(ix) {
            Some(*p)
        } else {
            self.bar_close.get(ix).and_then(|c| *c)
        }
    }

    /// Net base position for an instrument (zero if unregistered).
    pub fn position_qty(&self, inst: &InstrumentId) -> Decimal {
        self.registry
            .index_of(inst)
            .and_then(|ix| self.positions.get(ix.0).copied())
            .unwrap_or(Decimal::ZERO)
    }

    /// Attributed net base position for a sub-strategy on an instrument (barter-style `strategy_id` slice).
    pub fn strategy_position_qty(&self, inst: &InstrumentId, strategy_id: &StrategyId) -> Decimal {
        let Some(ix) = self.registry.index_of(inst).map(|i| i.0) else {
            return Decimal::ZERO;
        };
        self.strategy_positions
            .get(&(ix, strategy_id.clone()))
            .copied()
            .unwrap_or(Decimal::ZERO)
    }

    /// Alias for [`Self::strategy_position_qty`] (keyword-style naming for future bindings).
    #[inline]
    pub fn position_qty_for_strategy(
        &self,
        inst: &InstrumentId,
        strategy_id: &StrategyId,
    ) -> Decimal {
        self.strategy_position_qty(inst, strategy_id)
    }

    /// Update market state from a preloaded bar (tick-native replay).
    pub fn apply_bar(
        &mut self,
        ix: usize,
        bar: &crate::backtest::Bar,
        tick_size: rust_decimal::Decimal,
        _half_spread_bps: rust_decimal::Decimal,
    ) {
        let Some(ts) = bar.timestamp() else {
            return;
        };
        let open = crate::backtest::ticks_to_decimal(bar.open_ticks, tick_size);
        let high = crate::backtest::ticks_to_decimal(bar.high_ticks, tick_size);
        let low = crate::backtest::ticks_to_decimal(bar.low_ticks, tick_size);
        let close = crate::backtest::ticks_to_decimal(bar.close_ticks, tick_size);
        self.last_trade[ix] = Some((ts, close));
        self.bar_close[ix] = Some(close);
        self.bar_high[ix] = Some(high);
        self.bar_low[ix] = Some(low);
        self.last_event_ts = Some(ts);
        self.l1[ix] = Some((ts, low, high));
        let _ = open;
    }

    /// Apply a market event (read-only book/trade updates).
    pub fn apply_market(&mut self, ev: &MarketEvent) {
        match ev {
            MarketEvent::Trade {
                instrument,
                ts,
                price,
                ..
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|i| i.0) {
                    self.last_trade[ix] = Some((*ts, *price));
                    self.last_event_ts = Some(*ts);
                }
            }
            MarketEvent::BookL1 {
                instrument,
                ts,
                bid,
                ask,
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|i| i.0) {
                    self.l1[ix] = Some((*ts, *bid, *ask));
                    self.last_event_ts = Some(*ts);
                }
            }
            MarketEvent::BookL2Snapshot(snap) => {
                if let Some(ix) = self.registry.index_of(&snap.instrument).map(|i| i.0) {
                    self.l2[ix] = Some(snap.clone());
                }
            }
            MarketEvent::Bar {
                instrument,
                ts,
                open,
                high,
                low,
                close,
                ..
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|i| i.0) {
                    self.last_trade[ix] = Some((*ts, *close));
                    self.bar_close[ix] = Some(*close);
                    self.bar_high[ix] = Some(*high);
                    self.bar_low[ix] = Some(*low);
                    self.last_event_ts = Some(*ts);
                    self.l1[ix] = Some((*ts, *low, *high));
                    let _ = open;
                }
            }
        }
    }

    /// Mid price for a dense instrument row (no hash lookup).
    pub fn mid_or_last_ix(&self, ix: usize) -> Option<Decimal> {
        if let Some(Some((_, bid, ask))) = self.l1.get(ix) {
            Some((*bid + *ask) / Decimal::from(2u64))
        } else if let Some(Some((_, p))) = self.last_trade.get(ix) {
            Some(*p)
        } else {
            self.bar_close.get(ix).and_then(|c| *c)
        }
    }

    /// Mark-to-market equity for one instrument row.
    pub fn mark_to_market_equity_ix(&self, ix: usize) -> Option<Decimal> {
        let mid = self.mid_or_last_ix(ix)?;
        let meta = self.registry.meta(InstrumentIndex(ix))?;
        let base = self
            .balances
            .get(&meta.base)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let quote = self
            .balances
            .get(&meta.quote)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let notional = Self::position_exposure(meta, base, mid);
        Some(quote + notional)
    }

    /// Apply account event (balances, orders, fills).
    pub fn apply_account(&mut self, ev: &AccountEvent) {
        match ev {
            AccountEvent::Balance { asset, free } => {
                self.balances.insert(asset.clone(), *free);
            }
            AccountEvent::BalanceDelta { asset, delta } => {
                let entry = self.balances.entry(asset.clone()).or_insert(Decimal::ZERO);
                *entry += *delta;
            }
            AccountEvent::OrderUpdate {
                id,
                instrument,
                side,
                order_type,
                price,
                stop_price,
                remaining_qty,
                original_qty,
                status,
                strategy_id,
            } => {
                let o = OpenOrder {
                    id: id.clone(),
                    instrument: instrument.clone(),
                    side: *side,
                    order_type: *order_type,
                    price: *price,
                    stop_price: *stop_price,
                    remaining_qty: *remaining_qty,
                    original_qty: *original_qty,
                    status: *status,
                    strategy_id: strategy_id.clone(),
                };
                self.open_orders.apply_order_update(o);
            }
            AccountEvent::Fill {
                instrument,
                side,
                price,
                qty,
                fee,
                strategy_id,
                ..
            } => {
                let Some(ix) = self.registry.index_of(instrument).map(|i| i.0) else {
                    return;
                };
                let sign = match side {
                    Side::Buy => Decimal::ONE,
                    Side::Sell => -Decimal::ONE,
                };
                let delta = sign * qty;
                self.positions[ix] += delta;
                if let Some(ref sid) = strategy_id {
                    *self
                        .strategy_positions
                        .entry((ix, sid.clone()))
                        .or_insert(Decimal::ZERO) += delta;
                }
                self.fill_count += 1;
                if let Some(ts) = self.last_event_ts {
                    self.fill_log.push(FillRecord {
                        ts,
                        side: *side,
                        qty: qty.to_string(),
                        price: price.to_string(),
                        fee: fee.to_string(),
                        strategy_id: strategy_id.clone(),
                    });
                }
            }
        }
    }

    /// Mark-to-market equity in quote using mid or last trade.
    pub fn mark_to_market_equity(&self, inst: &InstrumentId) -> Option<Decimal> {
        let ix = self.registry.index_of(inst)?.0;
        self.mark_to_market_equity_ix(ix)
    }

    /// Snapshot for risk checks (cheap clone).
    pub fn snapshot(&self) -> GlobalState {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AccountEvent;
    use crate::types::Asset;
    use crate::types::{OrderId, Side, StrategyId};
    use rust_decimal::Decimal;
    use std::collections::HashMap;

    #[test]
    fn apply_bar_sets_l1_from_high_low() {
        let i = crate::types::InstrumentId::new("test", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let reg = InstrumentRegistry::from_instruments(inst);
        let mut state = GlobalState::new(reg, HashMap::new());
        let tick = crate::backtest::default_tick_size();
        let bar = crate::backtest::Bar {
            ts_unix_nanos: time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64,
            open_ticks: 9_900_000_000,
            high_ticks: 10_200_000_000,
            low_ticks: 9_800_000_000,
            close_ticks: 10_000_000_000,
            volume_lots: 1,
        };
        state.apply_bar(0, &bar, tick, Decimal::from(100u64));
        let (_, bid, ask) = state.l1[0].unwrap();
        assert_eq!(bid, Decimal::new(98, 0));
        assert_eq!(ask, Decimal::new(102, 0));
    }

    #[test]
    fn mark_equity_includes_cash() {
        let i = crate::types::InstrumentId::new("test", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(1000u64));
        let reg = InstrumentRegistry::from_instruments(inst);
        let s = GlobalState::new(reg, bal);
        assert_eq!(
            s.mark_equity_quote(&Asset("USDT".into())),
            Decimal::from(1000u64)
        );
    }

    #[test]
    fn strategy_positions_from_tagged_fills() {
        let i = crate::types::InstrumentId::new("test", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(1000u64));
        let reg = InstrumentRegistry::from_instruments(inst);
        let mut st = GlobalState::new(reg, bal);
        let sid_a = StrategyId::new("momentum");
        let sid_b = StrategyId::new("mean_rev");
        st.apply_account(&AccountEvent::Fill {
            order_id: OrderId::new_v4(),
            instrument: i.clone(),
            side: Side::Buy,
            price: Decimal::from(50u64),
            qty: Decimal::new(1, 1),
            fee: Decimal::ZERO,
            fee_asset: Asset("USDT".into()),
            strategy_id: Some(sid_a.clone()),
        });
        st.apply_account(&AccountEvent::Fill {
            order_id: OrderId::new_v4(),
            instrument: i.clone(),
            side: Side::Sell,
            price: Decimal::from(50u64),
            qty: Decimal::new(5, 2),
            fee: Decimal::ZERO,
            fee_asset: Asset("USDT".into()),
            strategy_id: Some(sid_b.clone()),
        });
        assert_eq!(st.position_qty(&i), Decimal::new(5, 2));
        assert_eq!(st.strategy_position_qty(&i, &sid_a), Decimal::new(1, 1));
        assert_eq!(st.strategy_position_qty(&i, &sid_b), -Decimal::new(5, 2));
        assert_eq!(st.position_qty_for_strategy(&i, &sid_a), Decimal::new(1, 1));
        assert_eq!(
            st.position_qty_for_strategy(&i, &sid_b),
            -Decimal::new(5, 2)
        );
    }
}
