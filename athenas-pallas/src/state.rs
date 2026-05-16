//! Authoritative in-memory book, balances, orders, and positions.

pub use crate::instrument::{InstrumentFilter, InstrumentIndex, InstrumentMeta, InstrumentRegistry};

use crate::events::{AccountEvent, BookL2Snapshot, MarketEvent};
use crate::oms::OrderStore;
use crate::types::{Asset, InstrumentId, OpenOrder, Side};
use rust_decimal::Decimal;
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
    /// Net base position per instrument row (signed).
    pub positions: Vec<Decimal>,
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
            l2: vec![None; n],
            risk_day_anchor: None,
            daily_risk_quote: None,
            paused: false,
            trading_state: crate::types::TradingState::Enabled,
        }
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
        let mut eq = *self.balances.get(quote).unwrap_or(&Decimal::ZERO);
        for (ix, pos) in self.positions.iter().enumerate() {
            if pos.is_zero() {
                continue;
            }
            let Some(inst) = self.registry.id(InstrumentIndex(ix)) else {
                continue;
            };
            let px = self.mid_or_last(inst).unwrap_or(Decimal::ZERO);
            eq += *pos * px;
        }
        eq
    }

    /// Mid price or last trade fallback.
    pub fn mid_or_last(&self, inst: &InstrumentId) -> Option<Decimal> {
        let ix = self.registry.index_of(inst)?.0;
        if let Some(Some((_, bid, ask))) = self.l1.get(ix) {
            Some((*bid + *ask) / Decimal::from(2u64))
        } else if let Some(Some((_, p))) = self.last_trade.get(ix) {
            Some(*p)
        } else {
            None
        }
    }

    /// Net base position for an instrument (zero if unregistered).
    pub fn position_qty(&self, inst: &InstrumentId) -> Decimal {
        self.registry
            .index_of(inst)
            .and_then(|ix| self.positions.get(ix.0).copied())
            .unwrap_or(Decimal::ZERO)
    }

    /// Apply a market event (read-only book/trade updates).
    pub fn apply_market(&mut self, ev: &MarketEvent) {
        match ev {
            MarketEvent::Trade {
                instrument, ts, price, ..
            } => {
                if let Some(ix) = self.registry.index_of(instrument).map(|i| i.0) {
                    self.last_trade[ix] = Some((*ts, *price));
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
                }
            }
            MarketEvent::BookL2Snapshot(snap) => {
                if let Some(ix) = self.registry.index_of(&snap.instrument).map(|i| i.0) {
                    self.l2[ix] = Some(snap.clone());
                }
            }
        }
    }

    /// Apply account event (balances, orders, fills).
    pub fn apply_account(&mut self, ev: &AccountEvent) {
        match ev {
            AccountEvent::Balance { asset, free } => {
                self.balances.insert(asset.clone(), *free);
            }
            AccountEvent::OrderUpdate {
                id,
                instrument,
                side,
                order_type,
                price,
                remaining_qty,
                original_qty,
                status,
            } => {
                let o = OpenOrder {
                    id: id.clone(),
                    instrument: instrument.clone(),
                    side: *side,
                    order_type: *order_type,
                    price: *price,
                    remaining_qty: *remaining_qty,
                    original_qty: *original_qty,
                    status: *status,
                };
                self.open_orders.apply_order_update(o);
            }
            AccountEvent::Fill {
                instrument,
                side,
                price: _,
                qty,
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
            }
        }
    }

    /// Mark-to-market equity in quote using mid or last trade.
    pub fn mark_to_market_equity(&self, inst: &InstrumentId) -> Option<Decimal> {
        let mid = self.mid_or_last(inst)?;
        let meta = self.registry.meta_by_id(inst)?;
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
        Some(quote + base * mid)
    }

    /// Snapshot for risk checks (cheap clone).
    pub fn snapshot(&self) -> GlobalState {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Asset;
    use rust_decimal::Decimal;
    use std::collections::HashMap;

    #[test]
    fn mark_equity_includes_cash() {
        let i = crate::types::InstrumentId::new("x", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            InstrumentMeta {
                base: Asset("BTC".into()),
                quote: Asset("USDT".into()),
            },
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(1000u64));
        let reg = InstrumentRegistry::from_instruments(inst);
        let s = GlobalState::new(reg, bal);
        assert_eq!(s.mark_equity_quote(&Asset("USDT".into())), Decimal::from(1000u64));
    }
}
