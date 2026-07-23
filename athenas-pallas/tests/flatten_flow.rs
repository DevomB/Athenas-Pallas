//! Flatten control path cancels then submits reduce-only market intents.

use athenas_pallas::dispatch_event_sync;
use athenas_pallas::events::{AccountEvent, ControlEvent, Event, MarketEvent, OrderIntent};
use athenas_pallas::execution::{PaperConfig, PaperExecution};
use athenas_pallas::risk::{PauseCheck, RiskEngine};
use athenas_pallas::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, InstrumentId, OrderId, OrderStatus, OrderType, Side};
use rust_decimal::Decimal;
use std::collections::HashMap;
use time::OffsetDateTime;

struct Quiet;

impl Strategy for Quiet {
    fn on_event(&mut self, _ctx: &StrategyContext<'_>, _event: &Event, _: &mut Vec<OrderIntent>) {}
}

#[test]
fn flatten_closes_position_when_paused() {
    let inst = InstrumentId::new("test", "BTCUSDT");
    let mut instruments = HashMap::new();
    instruments.insert(
        inst.clone(),
        InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
    );
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    balances.insert(Asset("BTC".into()), Decimal::new(1, 3));

    let registry = InstrumentRegistry::from_instruments(instruments);
    let mut state = GlobalState::new(registry, balances);
    if let Some(ix) = state.registry.index_of(&inst) {
        state.positions[ix.0] = Decimal::new(1, 3);
    }
    state.apply_account(&AccountEvent::OrderUpdate {
        id: OrderId::from_venue_u64(1),
        instrument: inst.clone(),
        side: Side::Buy,
        order_type: OrderType::Limit,
        price: Some(Decimal::ONE),
        stop_price: None,
        remaining_qty: Decimal::ONE,
        original_qty: Decimal::ONE,
        status: OrderStatus::Open,
        client_order_id: None,
        oco_group: None,
        strategy_id: None,
    });
    state.paused = true;

    let mut strat = Quiet;
    let risk = RiskEngine::new(vec![Box::new(PauseCheck)]);
    let exec = PaperExecution::new(PaperConfig::default());
    let mut intents = Vec::new();

    let ts = OffsetDateTime::now_utc();
    dispatch_event_sync(
        &mut state,
        &mut strat,
        &risk,
        &exec,
        Event::Market(MarketEvent::BookL1 {
            instrument: inst.clone(),
            ts,
            bid: Decimal::new(40_000, 0),
            ask: Decimal::new(40_010, 0),
            provenance: None,
        }),
        &mut intents,
    )
    .unwrap();

    dispatch_event_sync(
        &mut state,
        &mut strat,
        &risk,
        &exec,
        Event::Control(ControlEvent::Flatten),
        &mut intents,
    )
    .unwrap();

    assert!(state.position_qty(&inst).abs() < Decimal::new(1, 6));
    assert!(state.open_orders.is_empty());
}
