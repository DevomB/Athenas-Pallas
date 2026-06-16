use athenas_pallas::dispatch_event;
use athenas_pallas::events::{Event, MarketEvent, OrderIntent, OrderIntentSource};
use athenas_pallas::execution::{PaperConfig, PaperGateway};
use athenas_pallas::risk::{PauseCheck, RiskPipeline};
use athenas_pallas::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, InstrumentId, OrderType, Side};
use rust_decimal::Decimal;
use std::collections::HashMap;
use time::OffsetDateTime;

struct OneShot {
    inst: InstrumentId,
    fired: bool,
}

impl Strategy for OneShot {
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event, out: &mut Vec<OrderIntent>) {
        if self.fired {
            return;
        }
        if matches!(event, Event::Market(MarketEvent::BookL1 { .. })) {
            if ctx.state.mid_or_last(&self.inst).is_some() {
                self.fired = true;
                out.push(OrderIntent {
                    instrument: self.inst.clone(),
                    side: Side::Buy,
                    order_type: OrderType::Market,
                    price: None,
                    stop_price: None,
                    qty: Decimal::new(1, 3),
                    client_order_id: None,
                    source: OrderIntentSource::User,
                    strategy_id: None,
                });
            }
        }
    }
}

#[tokio::test]
async fn paper_market_updates_balances() {
    let inst = InstrumentId::new("binance", "BTCUSDT");
    let mut instruments = HashMap::new();
    instruments.insert(
        inst.clone(),
        InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
    );
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    balances.insert(Asset("BTC".into()), Decimal::ZERO);

    let registry = InstrumentRegistry::from_instruments(instruments);
    let mut state = GlobalState::new(registry, balances);
    let mut strat = OneShot {
        inst: inst.clone(),
        fired: false,
    };
    let risk = RiskPipeline::new(vec![Box::new(PauseCheck::default())]);
    let exec = PaperGateway::new(PaperConfig::default());

    let ts = OffsetDateTime::now_utc();
    let ev = Event::Market(MarketEvent::BookL1 {
        instrument: inst.clone(),
        ts,
        bid: Decimal::new(100_000, 0),
        ask: Decimal::new(100_010, 0),
    });
    dispatch_event(&mut state, &mut strat, &risk, &exec, ev)
        .await
        .unwrap();

    let btc = *state.balances.get(&Asset("BTC".into())).unwrap();
    assert!(btc > Decimal::ZERO);
}
