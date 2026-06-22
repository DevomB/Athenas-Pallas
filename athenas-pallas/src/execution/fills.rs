//! Sync fill simulation for backtest replay. local balances and simple touch / last-trade fill rules.

use std::sync::Arc;

use super::{apply_slippage, AccountEvents};
use crate::backtest::{FillModel, TouchCrossFillModel};
use crate::error::{Error, Result};
use crate::events::{AccountEvent, OrderIntent};
use crate::instrument::pricing::margin_required;
use crate::instrument::ticks::{notional_decimal, PriceTicks, QtyLots};
use crate::instrument::AssetClass;
use crate::state::{GlobalState, InstrumentMeta};
use crate::types::{OpenOrder, OrderId, OrderStatus, OrderType, Side};
use rust_decimal::Decimal;
use smallvec::smallvec;

/// Paper trading gateway configuration.
#[derive(Clone)]
pub struct PaperConfig {
    /// Taker fee in basis points.
    pub fee_bps: Decimal,
    /// Market order slippage in bps applied to mid/last.
    pub market_slippage_bps: Decimal,
    /// Limit crossing model.
    pub fill_model: Arc<dyn FillModel>,
}

impl std::fmt::Debug for PaperConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaperConfig")
            .field("fee_bps", &self.fee_bps)
            .field("market_slippage_bps", &self.market_slippage_bps)
            .field("fill_model", &self.fill_model.name())
            .finish()
    }
}

impl Default for PaperConfig {
    fn default() -> Self {
        Self {
            fee_bps: Decimal::from(10u64),
            market_slippage_bps: Decimal::from(5u64),
            fill_model: Arc::new(TouchCrossFillModel),
        }
    }
}

/// Sync fill engine for backtest and simulation gateways.
#[derive(Clone)]
pub struct FillEngine {
    cfg: PaperConfig,
}

impl FillEngine {
    /// New paper gateway.
    pub fn new(cfg: PaperConfig) -> Self {
        Self { cfg }
    }

    fn meta<'a>(
        state: &'a GlobalState,
        inst: &crate::types::InstrumentId,
    ) -> Result<&'a InstrumentMeta> {
        state
            .registry
            .meta_by_id(inst)
            .ok_or_else(|| Error::Invalid("unknown instrument".into()))
    }

    fn fee_notional(notional: Decimal, bps: Decimal) -> Decimal {
        notional * bps / Decimal::from(10_000u64)
    }

    fn notional(meta: &InstrumentMeta, price: Decimal, qty: Decimal) -> Decimal {
        let tick = meta.tick_size.unwrap_or(Decimal::new(1, 8));
        let lot = meta.lot_size.unwrap_or(Decimal::ONE);
        if PriceTicks::is_exact(price, tick) && QtyLots::is_exact(qty, lot) {
            if let (Some(pt), Some(ql)) = (
                PriceTicks::from_decimal(price, tick),
                QtyLots::from_decimal(qty, lot),
            ) {
                let base = notional_decimal(pt, ql, tick, lot);
                return match meta.asset_class {
                    AssetClass::Future | AssetClass::Perpetual | AssetClass::Option => {
                        base * meta.contract_multiplier.unwrap_or(Decimal::ONE)
                    }
                    _ => base,
                };
            }
        }
        match meta.asset_class {
            AssetClass::Future | AssetClass::Perpetual | AssetClass::Option => {
                let mult = meta.contract_multiplier.unwrap_or(Decimal::ONE);
                price * qty * mult
            }
            _ => price * qty,
        }
    }

    fn round_qty_to_lot(qty: Decimal, lot: Decimal) -> Decimal {
        if lot.is_zero() {
            return qty;
        }
        (qty / lot).floor() * lot
    }

    fn round_price_to_tick(price: Decimal, tick: Decimal) -> Decimal {
        if tick.is_zero() {
            return price;
        }
        (price / tick).round() * tick
    }

    fn normalize_order(
        meta: &InstrumentMeta,
        qty: Decimal,
        price: Option<Decimal>,
        stop_price: Option<Decimal>,
    ) -> Result<(Decimal, Option<Decimal>, Option<Decimal>)> {
        let qty = meta
            .lot_size
            .map(|lot| Self::round_qty_to_lot(qty, lot))
            .unwrap_or(qty);
        if qty.is_zero() {
            return Err(Error::Invalid("quantity below lot_size".into()));
        }
        let price = price.map(|p| {
            meta.tick_size
                .map(|tick| Self::round_price_to_tick(p, tick))
                .unwrap_or(p)
        });
        let stop_price = stop_price.map(|p| {
            meta.tick_size
                .map(|tick| Self::round_price_to_tick(p, tick))
                .unwrap_or(p)
        });
        Ok((qty, price, stop_price))
    }

    fn bar_high_low(
        state: &GlobalState,
        inst: &crate::types::InstrumentId,
    ) -> Option<(Decimal, Decimal)> {
        let ix = state.registry.index_of(inst)?.0;
        let high = state.bar_high.get(ix).and_then(|c| *c)?;
        let low = state.bar_low.get(ix).and_then(|c| *c)?;
        Some((high, low))
    }

    fn stop_triggered(side: Side, stop: Decimal, high: Decimal, low: Decimal) -> bool {
        match side {
            Side::Buy => high >= stop,
            Side::Sell => low <= stop,
        }
    }

    fn uses_derivative_margin(meta: &InstrumentMeta) -> bool {
        matches!(
            meta.asset_class,
            AssetClass::Future | AssetClass::Perpetual | AssetClass::Option
        )
    }

    /// Cash debited/credited in quote for a fill (before fees).
    fn quote_cash_flow(meta: &InstrumentMeta, side: Side, price: Decimal, qty: Decimal) -> Decimal {
        if Self::uses_derivative_margin(meta) {
            // Derivatives: equity is mark-to-market (cash + position * price * multiplier).
            // Opening/closing adjusts contracts only; cash moves on margin when configured.
            match meta.margin_initial_rate {
                Some(rate) if rate < Decimal::ONE => margin_required(meta, price, qty),
                _ => Decimal::ZERO,
            }
        } else {
            match side {
                Side::Buy => Self::notional(meta, price, qty),
                Side::Sell => -Self::notional(meta, price, qty),
            }
        }
    }

    fn check_balance_for_fill(
        state: &GlobalState,
        meta: &InstrumentMeta,
        side: Side,
        price: Decimal,
        qty: Decimal,
        fee: Decimal,
    ) -> Result<()> {
        let base_free = *state.balances.get(&meta.base).unwrap_or(&Decimal::ZERO);
        let quote_free = *state.balances.get(&meta.quote).unwrap_or(&Decimal::ZERO);
        let cash = Self::quote_cash_flow(meta, side, price, qty);
        match side {
            Side::Buy if quote_free < cash + fee => Err(Error::ExecutionRejected(
                "insufficient quote balance".into(),
            )),
            Side::Sell if !Self::uses_derivative_margin(meta) && base_free < qty => {
                Err(Error::ExecutionRejected("insufficient base balance".into()))
            }
            Side::Sell if Self::uses_derivative_margin(meta) && quote_free < cash + fee => Err(
                Error::ExecutionRejected("insufficient quote balance".into()),
            ),
            _ => Ok(()),
        }
    }

    fn build_open(
        id: OrderId,
        intent: &OrderIntent,
        qty: Decimal,
        price: Option<Decimal>,
        stop_price: Option<Decimal>,
        ot: OrderType,
    ) -> OpenOrder {
        OpenOrder {
            id,
            instrument: intent.instrument.clone(),
            side: intent.side,
            order_type: ot,
            price,
            stop_price,
            remaining_qty: qty,
            original_qty: qty,
            status: OrderStatus::Open,
            strategy_id: intent.strategy_id.clone(),
        }
    }

    fn emit_order_update(o: &OpenOrder) -> AccountEvent {
        AccountEvent::OrderUpdate {
            id: o.id.clone(),
            instrument: o.instrument.clone(),
            side: o.side,
            order_type: o.order_type,
            price: o.price,
            stop_price: o.stop_price,
            remaining_qty: o.remaining_qty,
            original_qty: o.original_qty,
            status: o.status,
            strategy_id: o.strategy_id.clone(),
        }
    }

    /// Append balance adjustment events after a fill (quote spent/received, fee).
    fn balance_updates_after_fill(
        state: &GlobalState,
        order: &OpenOrder,
        meta: &InstrumentMeta,
        price: Decimal,
        qty: Decimal,
        fee: Decimal,
    ) -> Vec<AccountEvent> {
        let mut evs = Vec::new();
        let base_free = *state.balances.get(&meta.base).unwrap_or(&Decimal::ZERO);
        let quote_free = *state.balances.get(&meta.quote).unwrap_or(&Decimal::ZERO);
        let cash = Self::quote_cash_flow(meta, order.side, price, qty);
        let (new_base, new_quote) = match order.side {
            Side::Buy => (base_free + qty, quote_free - cash - fee),
            Side::Sell => (base_free - qty, quote_free - cash - fee),
        };
        evs.push(AccountEvent::Balance {
            asset: meta.base.clone(),
            free: new_base,
        });
        evs.push(AccountEvent::Balance {
            asset: meta.quote.clone(),
            free: new_quote,
        });
        evs
    }

    fn crossing_limit(
        fill_model: &dyn FillModel,
        side: Side,
        limit: Decimal,
        bid: Decimal,
        ask: Decimal,
    ) -> Option<Decimal> {
        if fill_model.limit_would_fill(side, limit, bid, ask) {
            Some(match side {
                Side::Buy => ask,
                Side::Sell => bid,
            })
        } else {
            None
        }
    }

    fn emit_fill_events(
        &self,
        state: &GlobalState,
        order: &OpenOrder,
        meta: &InstrumentMeta,
        px: Decimal,
        qty: Decimal,
    ) -> Result<AccountEvents> {
        let fee = Self::fee_notional(Self::notional(meta, px, qty), self.cfg.fee_bps);
        Self::check_balance_for_fill(state, meta, order.side, px, qty, fee)?;
        let mut filled = order.clone();
        filled.remaining_qty = Decimal::ZERO;
        filled.status = OrderStatus::Filled;
        let mut evs: AccountEvents = smallvec![Self::emit_order_update(&filled)];
        evs.push(AccountEvent::Fill {
            order_id: filled.id.clone(),
            instrument: filled.instrument.clone(),
            side: filled.side,
            price: px,
            qty,
            fee,
            fee_asset: meta.quote.clone(),
            strategy_id: filled.strategy_id.clone(),
        });
        evs.extend(Self::balance_updates_after_fill(
            state, &filled, meta, px, qty, fee,
        ));
        Ok(evs)
    }

    fn l1_bid_ask(
        state: &GlobalState,
        inst: &crate::types::InstrumentId,
    ) -> Option<(Decimal, Decimal)> {
        let ix = state.registry.index_of(inst)?.0;
        state
            .l1
            .get(ix)
            .and_then(|cell| *cell)
            .map(|(_, bid, ask)| (bid, ask))
    }

    /// Sync limit placement (backtest hot path).
    pub(crate) fn place_limit_sync(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<AccountEvents> {
        if intent.order_type != OrderType::Limit {
            return Err(Error::Invalid("place_limit requires limit intent".into()));
        }
        let meta = Self::meta(state, &intent.instrument)?;
        let (qty, price, _) =
            Self::normalize_order(meta, intent.qty, intent.price, intent.stop_price)?;
        let price = price.ok_or_else(|| Error::Invalid("limit needs price".into()))?;
        let id = OrderId::new_v4();
        let o = Self::build_open(id, intent, qty, Some(price), None, OrderType::Limit);

        if let Some((bid, ask)) = Self::l1_bid_ask(state, &intent.instrument) {
            if let Some(px) =
                Self::crossing_limit(self.cfg.fill_model.as_ref(), intent.side, price, bid, ask)
            {
                return self.emit_fill_events(state, &o, meta, px, qty);
            }
        }

        Ok(smallvec![Self::emit_order_update(&o)])
    }

    /// Sync market placement.
    pub(crate) fn place_market_sync(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<AccountEvents> {
        let meta = Self::meta(state, &intent.instrument)?;
        let (qty, _, _) = Self::normalize_order(meta, intent.qty, intent.price, intent.stop_price)?;
        let mid = state
            .mid_or_last(&intent.instrument)
            .ok_or_else(|| Error::Invalid("no mid/last for market order".into()))?;
        let px = apply_slippage(intent.side, mid, self.cfg.market_slippage_bps);
        let id = OrderId::new_v4();
        let o = Self::build_open(id, intent, qty, None, None, OrderType::Market);
        self.emit_fill_events(state, &o, meta, px, qty)
    }

    /// Sync cancel.
    pub(crate) fn cancel_sync(
        &self,
        state: &GlobalState,
        order_id: OrderId,
    ) -> Result<AccountEvents> {
        let o = state
            .open_orders
            .get(&order_id)
            .cloned()
            .ok_or_else(|| Error::Invalid("unknown order".into()))?;
        let mut c = o;
        c.status = OrderStatus::Canceled;
        c.remaining_qty = Decimal::ZERO;
        Ok(smallvec![Self::emit_order_update(&c)])
    }

    /// Sync cancel all.
    pub(crate) fn cancel_all_sync(&self, state: &GlobalState) -> Result<AccountEvents> {
        let mut out = AccountEvents::new();
        for (_, o) in state.open_orders.iter() {
            let mut c = o.clone();
            c.status = OrderStatus::Canceled;
            c.remaining_qty = Decimal::ZERO;
            out.push(Self::emit_order_update(&c));
        }
        Ok(out)
    }

    /// Sync stop-market placement (rests until bar high/low crosses trigger).
    pub(crate) fn place_stop_market_sync(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<AccountEvents> {
        if intent.order_type != OrderType::StopMarket {
            return Err(Error::Invalid(
                "place_stop_market requires StopMarket".into(),
            ));
        }
        let meta = Self::meta(state, &intent.instrument)?;
        let stop = intent
            .stop_price
            .or(intent.price)
            .ok_or_else(|| Error::Invalid("stop market needs stop_price".into()))?;
        let (qty, _, stop_price) =
            Self::normalize_order(meta, intent.qty, intent.price, Some(stop))?;
        let stop_price = stop_price.unwrap();
        let id = OrderId::new_v4();
        let o = Self::build_open(
            id,
            intent,
            qty,
            None,
            Some(stop_price),
            OrderType::StopMarket,
        );

        if let Some((high, low)) = Self::bar_high_low(state, &intent.instrument) {
            if Self::stop_triggered(intent.side, stop_price, high, low) {
                let mid = state
                    .mid_or_last(&intent.instrument)
                    .ok_or_else(|| Error::Invalid("no mid for stop market fill".into()))?;
                let px = apply_slippage(intent.side, mid, self.cfg.market_slippage_bps);
                return self.emit_fill_events(state, &o, meta, px, qty);
            }
        }

        Ok(smallvec![Self::emit_order_update(&o)])
    }

    /// Sync stop-limit placement.
    pub(crate) fn place_stop_limit_sync(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<AccountEvents> {
        if intent.order_type != OrderType::StopLimit {
            return Err(Error::Invalid("place_stop_limit requires StopLimit".into()));
        }
        let meta = Self::meta(state, &intent.instrument)?;
        let limit = intent
            .price
            .ok_or_else(|| Error::Invalid("stop limit needs limit price".into()))?;
        let stop = intent
            .stop_price
            .ok_or_else(|| Error::Invalid("stop limit needs stop_price".into()))?;
        let (qty, price, stop_price) =
            Self::normalize_order(meta, intent.qty, Some(limit), Some(stop))?;
        let price = price.unwrap();
        let stop_price = stop_price.unwrap();
        let id = OrderId::new_v4();
        let o = Self::build_open(
            id,
            intent,
            qty,
            Some(price),
            Some(stop_price),
            OrderType::StopLimit,
        );

        if let Some((high, low)) = Self::bar_high_low(state, &intent.instrument) {
            if Self::stop_triggered(intent.side, stop_price, high, low) {
                if let Some((bid, ask)) = Self::l1_bid_ask(state, &intent.instrument) {
                    if let Some(px) = Self::crossing_limit(
                        self.cfg.fill_model.as_ref(),
                        intent.side,
                        price,
                        bid,
                        ask,
                    ) {
                        return self.emit_fill_events(state, &o, meta, px, qty);
                    }
                }
            }
        }

        Ok(smallvec![Self::emit_order_update(&o)])
    }

    /// Evaluate one resting order against current market data, appending any resulting fill events.
    ///
    /// Shared by the full-book and instrument-scoped pollers so both apply identical fill rules
    /// (limit cross via the configured [`crate::execution::FillModel`], stop trigger on bar
    /// high/low, then market/limit execution price).
    fn try_fill_resting(&self, state: &GlobalState, o: &OpenOrder, out: &mut AccountEvents) {
        let meta = match Self::meta(state, &o.instrument) {
            Ok(m) => m,
            Err(_) => return,
        };
        let qty = o.remaining_qty;

        match o.order_type {
            OrderType::Limit => {
                let Some(limit) = o.price else { return };
                let Some((bid, ask)) = Self::l1_bid_ask(state, &o.instrument) else {
                    return;
                };
                let Some(px) =
                    Self::crossing_limit(self.cfg.fill_model.as_ref(), o.side, limit, bid, ask)
                else {
                    return;
                };
                if let Ok(evs) = self.emit_fill_events(state, o, meta, px, qty) {
                    out.extend(evs);
                }
            }
            OrderType::StopMarket => {
                let Some(stop) = o.stop_price else { return };
                let Some((high, low)) = Self::bar_high_low(state, &o.instrument) else {
                    return;
                };
                if !Self::stop_triggered(o.side, stop, high, low) {
                    return;
                }
                let Some(mid) = state.mid_or_last(&o.instrument) else {
                    return;
                };
                let px = apply_slippage(o.side, mid, self.cfg.market_slippage_bps);
                if let Ok(evs) = self.emit_fill_events(state, o, meta, px, qty) {
                    out.extend(evs);
                }
            }
            OrderType::StopLimit => {
                let Some(stop) = o.stop_price else { return };
                let Some(limit) = o.price else { return };
                let Some((high, low)) = Self::bar_high_low(state, &o.instrument) else {
                    return;
                };
                if !Self::stop_triggered(o.side, stop, high, low) {
                    return;
                }
                let Some((bid, ask)) = Self::l1_bid_ask(state, &o.instrument) else {
                    return;
                };
                let Some(px) =
                    Self::crossing_limit(self.cfg.fill_model.as_ref(), o.side, limit, bid, ask)
                else {
                    return;
                };
                if let Ok(evs) = self.emit_fill_events(state, o, meta, px, qty) {
                    out.extend(evs);
                }
            }
            OrderType::Market => {}
        }
    }

    /// Sync passive limit fills after market data, scanning the whole book.
    pub(crate) fn poll_after_market_sync(&self, state: &GlobalState) -> Result<AccountEvents> {
        let mut out = AccountEvents::new();
        for instrument in state
            .open_orders
            .instruments_with_orders()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.poll_instrument_into(state, &instrument, &mut out)?;
        }
        Ok(out)
    }

    /// Sync passive fills restricted to the instrument that just ticked.
    pub(crate) fn poll_after_market_instrument_sync(
        &self,
        state: &GlobalState,
        instrument: &crate::types::InstrumentId,
    ) -> Result<AccountEvents> {
        let mut out = AccountEvents::new();
        self.poll_instrument_into(state, instrument, &mut out)?;
        Ok(out)
    }

    fn poll_instrument_into(
        &self,
        state: &GlobalState,
        instrument: &crate::types::InstrumentId,
        out: &mut AccountEvents,
    ) -> Result<()> {
        let (bid, ask) = match Self::l1_bid_ask(state, instrument) {
            Some((b, a)) => (Some(b), Some(a)),
            None => (None, None),
        };
        let (high, low) = match Self::bar_high_low(state, instrument) {
            Some((h, l)) => (Some(h), Some(l)),
            None => (None, None),
        };
        for id in state
            .open_orders
            .pollable_ids(instrument, bid, ask, high, low)
        {
            let Some(o) = state.open_orders.get(&id) else {
                continue;
            };
            self.try_fill_resting(state, o, out);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AccountEvent;
    use crate::events::MarketEvent;
    use crate::instrument::InstrumentRegistry;
    use crate::state::GlobalState;
    use crate::types::{Asset, InstrumentId};
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use time::OffsetDateTime;

    fn setup_l1(bid: Decimal, ask: Decimal) -> (GlobalState, InstrumentId) {
        let inst = InstrumentId::new("test", "BTCUSDT");
        let mut instruments = HashMap::new();
        instruments.insert(
            inst.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut balances = HashMap::new();
        balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
        let mut state =
            GlobalState::new(InstrumentRegistry::from_instruments(instruments), balances);
        state.apply_market(&MarketEvent::BookL1 {
            instrument: inst.clone(),
            ts: OffsetDateTime::now_utc(),
            bid,
            ask,
        });
        (state, inst)
    }

    #[test]
    fn buy_limit_at_ask_fills() {
        let (state, inst) = setup_l1(Decimal::new(100, 0), Decimal::new(101, 0));
        let gw = FillEngine::new(PaperConfig::default());
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Some(Decimal::new(101, 0)),
            stop_price: None,
            qty: Decimal::new(1, 2),
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let evs = gw.place_limit_sync(&state, &intent).unwrap();
        assert!(evs.iter().any(|e| matches!(e, AccountEvent::Fill { .. })));
    }

    #[test]
    fn market_fill_updates_balances_and_fee() {
        let inst = InstrumentId::new("test", "BTCUSDT");
        let mut instruments = HashMap::new();
        instruments.insert(
            inst.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut balances = HashMap::new();
        balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
        let mut st = GlobalState::new(InstrumentRegistry::from_instruments(instruments), balances);
        st.apply_market(&MarketEvent::BookL1 {
            instrument: inst.clone(),
            ts: OffsetDateTime::now_utc(),
            bid: Decimal::new(100, 0),
            ask: Decimal::new(101, 0),
        });
        let gw = FillEngine::new(PaperConfig {
            fee_bps: Decimal::from(100u64),
            market_slippage_bps: Decimal::ZERO,
            ..PaperConfig::default()
        });
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let evs = gw.place_market_sync(&st, &intent).unwrap();
        for ev in evs {
            st.apply_account(&ev);
        }
        let quote = *st.balances.get(&Asset("USDT".into())).unwrap();
        assert!(quote < Decimal::new(10_000, 0));
        let base = *st
            .balances
            .get(&Asset("BTC".into()))
            .unwrap_or(&Decimal::ZERO);
        assert_eq!(base, Decimal::ONE);
    }

    #[test]
    fn slippage_moves_fill_price() {
        let (state, inst) = setup_l1(Decimal::new(100, 0), Decimal::new(100, 0));
        let low = FillEngine::new(PaperConfig {
            market_slippage_bps: Decimal::ZERO,
            ..PaperConfig::default()
        });
        let high = FillEngine::new(PaperConfig {
            market_slippage_bps: Decimal::from(100u64),
            ..PaperConfig::default()
        });
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let low_fill = low
            .place_market_sync(&state, &intent)
            .unwrap()
            .into_iter()
            .find_map(|e| match e {
                AccountEvent::Fill { price, .. } => Some(price),
                _ => None,
            })
            .unwrap();
        let high_fill = high
            .place_market_sync(&state, &intent)
            .unwrap()
            .into_iter()
            .find_map(|e| match e {
                AccountEvent::Fill { price, .. } => Some(price),
                _ => None,
            })
            .unwrap();
        assert!(high_fill > low_fill);
    }

    #[test]
    fn buy_limit_below_bid_does_not_fill() {
        let (state, inst) = setup_l1(Decimal::new(100, 0), Decimal::new(101, 0));
        let gw = FillEngine::new(PaperConfig::default());
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Some(Decimal::new(99, 0)),
            stop_price: None,
            qty: Decimal::new(1, 2),
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let evs = gw.place_limit_sync(&state, &intent).unwrap();
        assert!(!evs.iter().any(|e| matches!(e, AccountEvent::Fill { .. })));
    }

    #[test]
    fn market_buy_rejects_insufficient_quote() {
        let inst = InstrumentId::new("test", "BTCUSDT");
        let mut instruments = HashMap::new();
        instruments.insert(
            inst.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut balances = HashMap::new();
        balances.insert(Asset("USDT".into()), Decimal::ONE);
        let mut state =
            GlobalState::new(InstrumentRegistry::from_instruments(instruments), balances);
        state.apply_market(&MarketEvent::BookL1 {
            instrument: inst.clone(),
            ts: OffsetDateTime::now_utc(),
            bid: Decimal::new(40_000, 0),
            ask: Decimal::new(40_010, 0),
        });
        let gw = FillEngine::new(PaperConfig::default());
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let err = gw.place_market_sync(&state, &intent).unwrap_err();
        assert!(matches!(err, Error::ExecutionRejected(_)));
    }
}
