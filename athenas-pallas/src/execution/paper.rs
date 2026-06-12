//! Paper execution: local balances and simple touch / last-trade fill rules.

use std::sync::Arc;

use super::{apply_slippage, ExecutionGateway};
use crate::backtest::{FillModel, TouchCrossFillModel};
use crate::error::{Error, Result};
use crate::events::{AccountEvent, OrderIntent};
use crate::state::{GlobalState, InstrumentMeta};
use crate::types::{OpenOrder, OrderId, OrderStatus, OrderType, Side};
use async_trait::async_trait;
use rust_decimal::Decimal;

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

/// Thread-safe paper gateway (shared across engine tasks).
#[derive(Clone)]
pub struct PaperGateway {
    cfg: PaperConfig,
}

impl PaperGateway {
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

    fn build_open(
        id: OrderId,
        intent: &OrderIntent,
        qty: Decimal,
        price: Option<Decimal>,
        ot: OrderType,
    ) -> OpenOrder {
        OpenOrder {
            id,
            instrument: intent.instrument.clone(),
            side: intent.side,
            order_type: ot,
            price,
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
        let (new_base, new_quote) = match order.side {
            Side::Buy => {
                let cost = price * qty + fee;
                (base_free + qty, quote_free - cost)
            }
            Side::Sell => {
                let proceeds = price * qty - fee;
                (base_free - qty, quote_free + proceeds)
            }
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

    /// Sync limit placement (backtest hot path).
    pub(crate) fn place_limit_sync(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        if intent.order_type != OrderType::Limit {
            return Err(Error::Invalid("place_limit requires limit intent".into()));
        }
        let price = intent
            .price
            .ok_or_else(|| Error::Invalid("limit needs price".into()))?;
        let meta = Self::meta(state, &intent.instrument)?;
        let id = OrderId::new_v4();
        let mut o = Self::build_open(id, intent, intent.qty, Some(price), OrderType::Limit);

        if let Some(ix) = state.registry.index_of(&intent.instrument).map(|i| i.0) {
            if let Some(Some((_, bid, ask))) = state.l1.get(ix) {
                if let Some(px) = Self::crossing_limit(
                    self.cfg.fill_model.as_ref(),
                    intent.side,
                    price,
                    *bid,
                    *ask,
                ) {
                    o.remaining_qty = Decimal::ZERO;
                    o.status = OrderStatus::Filled;
                    let mut evs = vec![Self::emit_order_update(&o)];
                    let fee = Self::fee_notional(px * intent.qty, self.cfg.fee_bps);
                    evs.push(AccountEvent::Fill {
                        order_id: o.id.clone(),
                        instrument: o.instrument.clone(),
                        side: o.side,
                        price: px,
                        qty: intent.qty,
                        fee,
                        fee_asset: meta.quote.clone(),
                        strategy_id: o.strategy_id.clone(),
                    });
                    evs.extend(Self::balance_updates_after_fill(
                        state, &o, meta, px, intent.qty, fee,
                    ));
                    return Ok(evs);
                }
            }
        }

        Ok(vec![Self::emit_order_update(&o)])
    }

    /// Sync market placement.
    pub(crate) fn place_market_sync(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        let meta = Self::meta(state, &intent.instrument)?;
        let mid = state
            .mid_or_last(&intent.instrument)
            .ok_or_else(|| Error::Invalid("no mid/last for market order".into()))?;
        let px = apply_slippage(intent.side, mid, self.cfg.market_slippage_bps);
        let id = OrderId::new_v4();
        let o = Self::build_open(id, intent, intent.qty, None, OrderType::Market);
        let mut o_filled = o.clone();
        o_filled.remaining_qty = Decimal::ZERO;
        o_filled.status = OrderStatus::Filled;
        let fee = Self::fee_notional(px * intent.qty, self.cfg.fee_bps);
        let mut evs = vec![Self::emit_order_update(&o_filled)];
        evs.push(AccountEvent::Fill {
            order_id: o_filled.id.clone(),
            instrument: o_filled.instrument.clone(),
            side: o_filled.side,
            price: px,
            qty: intent.qty,
            fee,
            fee_asset: meta.quote.clone(),
            strategy_id: o_filled.strategy_id.clone(),
        });
        evs.extend(Self::balance_updates_after_fill(
            state, &o_filled, meta, px, intent.qty, fee,
        ));
        Ok(evs)
    }

    /// Sync cancel.
    pub(crate) fn cancel_sync(
        &self,
        state: &GlobalState,
        order_id: OrderId,
    ) -> Result<Vec<AccountEvent>> {
        let o = state
            .open_orders
            .get(&order_id)
            .cloned()
            .ok_or_else(|| Error::Invalid("unknown order".into()))?;
        let mut c = o;
        c.status = OrderStatus::Canceled;
        c.remaining_qty = Decimal::ZERO;
        Ok(vec![Self::emit_order_update(&c)])
    }

    /// Sync cancel all.
    pub(crate) fn cancel_all_sync(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        let mut out = Vec::new();
        for (_, o) in state.open_orders.iter() {
            let mut c = o.clone();
            c.status = OrderStatus::Canceled;
            c.remaining_qty = Decimal::ZERO;
            out.push(Self::emit_order_update(&c));
        }
        Ok(out)
    }

    /// Sync passive limit fills after market data.
    pub(crate) fn poll_after_market_sync(
        &self,
        state: &GlobalState,
    ) -> Result<Vec<AccountEvent>> {
        let mut out = Vec::new();
        let orders: Vec<OpenOrder> = state.open_orders.values().cloned().collect();
        for o in orders {
            if o.order_type != OrderType::Limit {
                continue;
            }
            let limit = match o.price {
                Some(p) => p,
                None => continue,
            };
            let last = state
                .registry
                .index_of(&o.instrument)
                .and_then(|ix| state.last_trade.get(ix.0).and_then(|cell| *cell))
                .map(|(_, p)| p);
            let Some(last_px) = last else { continue };
            let meta = match Self::meta(state, &o.instrument) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let fill = match o.side {
                Side::Buy if last_px <= limit => Some(last_px),
                Side::Sell if last_px >= limit => Some(last_px),
                _ => None,
            };
            if let Some(px) = fill {
                let qty = o.remaining_qty;
                let fee = Self::fee_notional(px * qty, self.cfg.fee_bps);
                let mut filled = o.clone();
                filled.remaining_qty = Decimal::ZERO;
                filled.status = OrderStatus::Filled;
                out.push(Self::emit_order_update(&filled));
                out.push(AccountEvent::Fill {
                    order_id: filled.id.clone(),
                    instrument: filled.instrument.clone(),
                    side: filled.side,
                    price: px,
                    qty,
                    fee,
                    fee_asset: meta.quote.clone(),
                    strategy_id: filled.strategy_id.clone(),
                });
                out.extend(Self::balance_updates_after_fill(
                    state, &filled, meta, px, qty, fee,
                ));
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl ExecutionGateway for PaperGateway {
    async fn place_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>> {
        self.place_limit_sync(state, intent)
    }

    async fn place_market(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>> {
        self.place_market_sync(state, intent)
    }

    async fn cancel(&self, state: &GlobalState, order_id: OrderId) -> Result<Vec<AccountEvent>> {
        self.cancel_sync(state, order_id)
    }

    async fn cancel_all(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        self.cancel_all_sync(state)
    }

    async fn poll_after_market(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        self.poll_after_market_sync(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::MarketEvent;
    use crate::events::AccountEvent;
    use crate::instrument::InstrumentRegistry;
    use crate::state::GlobalState;
    use crate::types::{Asset, InstrumentId};
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use time::OffsetDateTime;

    fn setup_l1(bid: Decimal, ask: Decimal) -> (GlobalState, InstrumentId) {
        let inst = InstrumentId::new("binance", "BTCUSDT");
        let mut instruments = HashMap::new();
        instruments.insert(
            inst.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut state = GlobalState::new(InstrumentRegistry::from_instruments(instruments), HashMap::new());
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
        let gw = PaperGateway::new(PaperConfig::default());
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Some(Decimal::new(101, 0)),
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
        let inst = InstrumentId::new("binance", "BTCUSDT");
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
        let gw = PaperGateway::new(PaperConfig {
            fee_bps: Decimal::from(100u64),
            market_slippage_bps: Decimal::ZERO,
            ..PaperConfig::default()
        });
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
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
        let base = *st.balances.get(&Asset("BTC".into())).unwrap_or(&Decimal::ZERO);
        assert_eq!(base, Decimal::ONE);
    }

    #[test]
    fn slippage_moves_fill_price() {
        let (state, inst) = setup_l1(Decimal::new(100, 0), Decimal::new(100, 0));
        let low = PaperGateway::new(PaperConfig {
            market_slippage_bps: Decimal::ZERO,
            ..PaperConfig::default()
        });
        let high = PaperGateway::new(PaperConfig {
            market_slippage_bps: Decimal::from(100u64),
            ..PaperConfig::default()
        });
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
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
        let gw = PaperGateway::new(PaperConfig::default());
        let intent = OrderIntent {
            instrument: inst,
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: Some(Decimal::new(99, 0)),
            qty: Decimal::new(1, 2),
            client_order_id: None,
            source: crate::events::OrderIntentSource::User,
            strategy_id: None,
        };
        let evs = gw.place_limit_sync(&state, &intent).unwrap();
        assert!(!evs.iter().any(|e| matches!(e, AccountEvent::Fill { .. })));
    }
}
