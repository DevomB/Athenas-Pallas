//! Sync fill simulation for backtest replay. local balances and simple touch / last-trade fill rules.

use std::sync::Arc;

use super::{apply_slippage, AccountEvents};
use super::{FillModel, TouchCrossFillModel};
use crate::error::{Error, Result};
use crate::events::{AccountEvent, OrderIntent, RejectionKind, RejectionRecord};
use crate::instrument::pricing::margin_required;
use crate::instrument::ticks::{notional_decimal, PriceTicks, QtyLots};
use crate::instrument::AssetClass;
use crate::state::{GlobalState, InstrumentMeta};
use crate::types::{OpenOrder, OrderId, OrderStatus, OrderType, Side};
use rust_decimal::prelude::Signed;
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
        matches!(meta.asset_class, AssetClass::Future | AssetClass::Perpetual)
    }

    fn quote_cash_flow(meta: &InstrumentMeta, side: Side, price: Decimal, qty: Decimal) -> Decimal {
        match side {
            Side::Buy => Self::notional(meta, price, qty),
            Side::Sell => -Self::notional(meta, price, qty),
        }
    }

    fn realized_derivative_pnl(
        state: &GlobalState,
        meta: &InstrumentMeta,
        instrument: &crate::types::InstrumentId,
        side: Side,
        price: Decimal,
        qty: Decimal,
    ) -> Decimal {
        let Some(ix) = state.registry.index_of(instrument).map(|index| index.0) else {
            return Decimal::ZERO;
        };
        let current = state.positions[ix];
        let delta = if side == Side::Buy { qty } else { -qty };
        if current.is_zero() || current.signum() == delta.signum() {
            return Decimal::ZERO;
        }
        let close_qty = current.abs().min(qty);
        let entry = state.average_entry_price[ix].unwrap_or(price);
        let multiplier = meta.contract_multiplier.unwrap_or(Decimal::ONE);
        (price - entry) * current.signum() * close_qty * multiplier
    }

    fn check_balance_for_fill(
        state: &GlobalState,
        instrument: &crate::types::InstrumentId,
        meta: &InstrumentMeta,
        side: Side,
        price: Decimal,
        qty: Decimal,
        fee: Decimal,
    ) -> Result<()> {
        let base_free = *state.balances.get(&meta.base).unwrap_or(&Decimal::ZERO);
        let quote_free = *state.balances.get(&meta.quote).unwrap_or(&Decimal::ZERO);
        if Self::uses_derivative_margin(meta) {
            let current = state.position_qty(instrument);
            let delta = if side == Side::Buy { qty } else { -qty };
            let projected = current + delta;
            let realized = Self::realized_derivative_pnl(state, meta, instrument, side, price, qty);
            let available = quote_free + realized;
            let opens_exposure = current.is_zero()
                || current.signum() == delta.signum()
                || (!projected.is_zero() && projected.signum() != current.signum());
            if meta.margin_initial_rate.is_none() {
                // Ponytail: definitions without a rate keep legacy no-margin behavior; import
                // point-in-time contract metadata before relying on leverage constraints.
                return (available >= fee).then_some(()).ok_or_else(|| {
                    Error::ExecutionRejected("insufficient quote balance for fee".into())
                });
            }
            if !opens_exposure {
                return (available >= fee).then_some(()).ok_or_else(|| {
                    Error::ExecutionRejected("insufficient quote balance for fee".into())
                });
            }
            let required = margin_required(meta, price, projected);
            return (available >= required + fee)
                .then_some(())
                .ok_or_else(|| Error::ExecutionRejected("insufficient derivative margin".into()));
        }
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
            client_order_id: intent.client_order_id.clone(),
            oco_group: intent.oco_group.clone(),
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
            client_order_id: o.client_order_id.clone(),
            oco_group: o.oco_group.clone(),
            strategy_id: o.strategy_id.clone(),
        }
    }

    /// Append balance adjustment events after a fill (quote spent/received, fee).
    fn push_balance_updates_after_fill(
        state: &GlobalState,
        order: &OpenOrder,
        meta: &InstrumentMeta,
        price: Decimal,
        qty: Decimal,
        fee: Decimal,
        evs: &mut AccountEvents,
    ) {
        let base_free = *state.balances.get(&meta.base).unwrap_or(&Decimal::ZERO);
        let quote_free = *state.balances.get(&meta.quote).unwrap_or(&Decimal::ZERO);
        let delta = if order.side == Side::Buy { qty } else { -qty };
        let new_base = base_free + delta;
        let new_quote = if Self::uses_derivative_margin(meta) {
            quote_free
                + Self::realized_derivative_pnl(
                    state,
                    meta,
                    &order.instrument,
                    order.side,
                    price,
                    qty,
                )
                - fee
        } else {
            quote_free - Self::quote_cash_flow(meta, order.side, price, qty) - fee
        };
        evs.push(AccountEvent::Balance {
            asset: meta.base.clone(),
            free: new_base,
        });
        evs.push(AccountEvent::Balance {
            asset: meta.quote.clone(),
            free: new_quote,
        });
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
        Self::check_balance_for_fill(state, &order.instrument, meta, order.side, px, qty, fee)?;
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
            client_order_id: filled.client_order_id.clone(),
            oco_group: filled.oco_group.clone(),
            strategy_id: filled.strategy_id.clone(),
        });
        self.push_oco_cancellations(state, &filled, &mut evs);
        Self::push_balance_updates_after_fill(state, &filled, meta, px, qty, fee, &mut evs);
        Ok(evs)
    }

    fn push_oco_cancellations(
        &self,
        state: &GlobalState,
        filled: &OpenOrder,
        evs: &mut AccountEvents,
    ) {
        let Some(group) = filled.oco_group.as_deref() else {
            return;
        };
        for sibling in state
            .open_orders
            .values()
            .filter(|order| order.id != filled.id && order.oco_group.as_deref() == Some(group))
        {
            let mut canceled = sibling.clone();
            canceled.status = OrderStatus::Canceled;
            canceled.remaining_qty = Decimal::ZERO;
            evs.push(Self::emit_order_update(&canceled));
        }
    }

    fn reject_resting_order(
        state: &GlobalState,
        order: &OpenOrder,
        error: &Error,
        out: &mut AccountEvents,
    ) {
        let mut rejected = order.clone();
        rejected.status = OrderStatus::Rejected;
        rejected.remaining_qty = Decimal::ZERO;
        out.push(Self::emit_order_update(&rejected));
        out.push(AccountEvent::Rejection(RejectionRecord {
            ts: state
                .last_event_ts
                .unwrap_or(time::OffsetDateTime::UNIX_EPOCH),
            kind: RejectionKind::Execution,
            instrument: order.instrument.clone(),
            client_order_id: order.client_order_id.clone(),
            reason: error.to_string(),
        }));
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

    fn market_reference(
        state: &GlobalState,
        inst: &crate::types::InstrumentId,
        side: Side,
    ) -> Option<Decimal> {
        Self::l1_bid_ask(state, inst)
            .map(|(bid, ask)| match side {
                Side::Buy => ask,
                Side::Sell => bid,
            })
            .or_else(|| state.mid_or_last(inst))
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
        let reference = Self::market_reference(state, &intent.instrument, intent.side)
            .ok_or_else(|| Error::Invalid("no mid/last for market order".into()))?;
        let px = apply_slippage(intent.side, reference, self.cfg.market_slippage_bps);
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
        for o in state.open_orders.values() {
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
                let reference = Self::market_reference(state, &intent.instrument, intent.side)
                    .ok_or_else(|| Error::Invalid("no mid for stop market fill".into()))?;
                let px = apply_slippage(intent.side, reference, self.cfg.market_slippage_bps);
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
        let price = match o.order_type {
            OrderType::Limit => self.resting_limit_price(state, o),
            OrderType::StopMarket => self.resting_stop_market_price(state, o),
            OrderType::StopLimit => self.resting_stop_limit_price(state, o),
            OrderType::Market => None,
        };
        let (Ok(meta), Some(price)) = (Self::meta(state, &o.instrument), price) else {
            return;
        };
        match self.emit_fill_events(state, o, meta, price, o.remaining_qty) {
            Ok(events) => out.extend(events),
            Err(error) => Self::reject_resting_order(state, o, &error, out),
        }
    }

    fn resting_limit_price(&self, state: &GlobalState, order: &OpenOrder) -> Option<Decimal> {
        let limit = order.price?;
        let bar_touch =
            Self::bar_high_low(state, &order.instrument).and_then(|(high, low)| match order.side {
                Side::Buy if low <= limit => Some(limit),
                Side::Sell if high >= limit => Some(limit),
                _ => None,
            });
        bar_touch.or_else(|| {
            Self::l1_bid_ask(state, &order.instrument).and_then(|(bid, ask)| {
                Self::crossing_limit(self.cfg.fill_model.as_ref(), order.side, limit, bid, ask)
            })
        })
    }

    fn resting_stop_market_price(&self, state: &GlobalState, order: &OpenOrder) -> Option<Decimal> {
        let stop = order.stop_price?;
        let (high, low) = Self::bar_high_low(state, &order.instrument)?;
        if !Self::stop_triggered(order.side, stop, high, low) {
            return None;
        }
        let reference = Self::market_reference(state, &order.instrument, order.side)?;
        Some(apply_slippage(
            order.side,
            reference,
            self.cfg.market_slippage_bps,
        ))
    }

    fn resting_stop_limit_price(&self, state: &GlobalState, order: &OpenOrder) -> Option<Decimal> {
        let stop = order.stop_price?;
        let limit = order.price?;
        let (high, low) = Self::bar_high_low(state, &order.instrument)?;
        if !Self::stop_triggered(order.side, stop, high, low) {
            return None;
        }
        let (bid, ask) = Self::l1_bid_ask(state, &order.instrument)?;
        Self::crossing_limit(self.cfg.fill_model.as_ref(), order.side, limit, bid, ask)
    }

    /// Sync passive limit fills after market data, scanning the whole book.
    pub(crate) fn poll_after_market_sync(&self, state: &GlobalState) -> Result<AccountEvents> {
        let mut out = AccountEvents::new();
        for instrument in state.open_orders.instruments_with_orders() {
            self.poll_instrument_into(state, instrument, &mut out)?;
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
        let (mut bid, mut ask) = match Self::l1_bid_ask(state, instrument) {
            Some((b, a)) => (Some(b), Some(a)),
            None => (None, None),
        };
        let (high, low) = match Self::bar_high_low(state, instrument) {
            Some((h, l)) => (Some(h), Some(l)),
            None => (None, None),
        };
        // Bar limits are eligible on an intrabar touch even when the completed close is elsewhere.
        bid = high.or(bid);
        ask = low.or(ask);
        let mut candidate_ids = crate::oms::OrderIdBuffer::new();
        state
            .open_orders
            .pollable_ids_into(instrument, bid, ask, high, low, &mut candidate_ids);
        let mut filled_oco_groups = rustc_hash::FxHashSet::default();
        for id in &candidate_ids {
            let Some(o) = state.open_orders.get(id) else {
                continue;
            };
            if o.oco_group
                .as_ref()
                .is_some_and(|group| filled_oco_groups.contains(group))
            {
                continue;
            }
            let before = out.len();
            self.try_fill_resting(state, o, out);
            if out[before..]
                .iter()
                .any(|event| matches!(event, AccountEvent::Fill { .. }))
            {
                if let Some(group) = &o.oco_group {
                    filled_oco_groups.insert(group.clone());
                }
            }
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
            provenance: None,
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
            oco_group: None,
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
            provenance: None,
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
            oco_group: None,
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
            oco_group: None,
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
            oco_group: None,
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
            provenance: None,
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
            oco_group: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let err = gw.place_market_sync(&state, &intent).unwrap_err();
        assert!(matches!(err, Error::ExecutionRejected(_)));
    }

    #[test]
    fn oco_fill_cancels_sibling_and_prevents_double_fill() {
        let (mut state, inst) = setup_l1(Decimal::new(100, 0), Decimal::new(101, 0));
        let gateway = FillEngine::new(PaperConfig {
            fee_bps: Decimal::ZERO,
            market_slippage_bps: Decimal::ZERO,
            ..PaperConfig::default()
        });
        for client_id in ["take-profit-a", "take-profit-b"] {
            let intent = OrderIntent {
                instrument: inst.clone(),
                side: Side::Buy,
                order_type: OrderType::Limit,
                price: Some(Decimal::from(99u64)),
                stop_price: None,
                qty: Decimal::ONE,
                client_order_id: Some(crate::types::ClientOrderId(client_id.into())),
                oco_group: Some("bracket-1".into()),
                source: crate::events::OrderIntentSource::User,
                strategy_id: None,
            };
            for event in gateway.place_limit_sync(&state, &intent).unwrap() {
                state.apply_account(&event);
            }
        }
        state.apply_market(&MarketEvent::BookL1 {
            instrument: inst.clone(),
            ts: OffsetDateTime::now_utc(),
            bid: Decimal::from(98u64),
            ask: Decimal::from(99u64),
            provenance: None,
        });

        let events = gateway
            .poll_after_market_instrument_sync(&state, &inst)
            .unwrap();

        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AccountEvent::Fill { .. }))
                .count(),
            1
        );
        assert!(events.iter().any(|event| matches!(
            event,
            AccountEvent::OrderUpdate {
                status: OrderStatus::Canceled,
                ..
            }
        )));
    }
}
