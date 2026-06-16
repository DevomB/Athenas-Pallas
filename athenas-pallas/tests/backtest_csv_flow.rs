//! End-to-end CSV replay with simulated fills.

mod common;

use athenas_pallas::backtest::{BuyAndHold, CsvBarSource, HistoricalSource};
use athenas_pallas::dispatch_event_sync;
use athenas_pallas::events::Event;
use athenas_pallas::execution::{PaperConfig, SyncPaperGateway};
use athenas_pallas::metrics::summarize;
use athenas_pallas::risk::{PauseCheck, RiskPipeline};
use athenas_pallas::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::types::{Asset, EquityPoint, ExchangeId, InstrumentId, Symbol};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

fn sample_csv() -> PathBuf {
    common::fixture("BTCUSDT_1d.csv")
}

#[test]
fn csv_replay_buy_and_hold() {
    let instrument = InstrumentId::new("binance", "BTCUSDT");
    let mut instruments = HashMap::new();
    instruments.insert(
        instrument.clone(),
        InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
    );
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    balances.insert(Asset("BTC".into()), Decimal::ZERO);

    let registry = InstrumentRegistry::from_instruments(instruments);
    let mut state = GlobalState::new(registry, balances);
    let qty = Decimal::from_f64(0.01).unwrap_or(Decimal::ZERO);
    let mut strategy = BuyAndHold::new(instrument.clone(), qty);
    let risk = RiskPipeline::new(vec![Box::new(PauseCheck::default())]);
    let exec = SyncPaperGateway::new(PaperConfig::default());

    let mut src = CsvBarSource::from_path(
        &sample_csv(),
        ExchangeId("binance".into()),
        Symbol("BTCUSDT".into()),
    )
    .expect("csv");

    let mut curve: Vec<EquityPoint> = Vec::new();
    let mut intents = Vec::new();
    while let Some(ev) = src.next_event() {
        let ts = match &ev {
            Event::Market(athenas_pallas::events::MarketEvent::Bar { ts, .. }) => *ts,
            _ => time::OffsetDateTime::now_utc(),
        };
        dispatch_event_sync(&mut state, &mut strategy, &risk, &exec, ev, &mut intents)
            .expect("dispatch");
        if let Some(eq) = state.mark_to_market_equity(&instrument) {
            curve.push(EquityPoint {
                ts,
                equity_quote: eq,
            });
        }
    }

    assert_eq!(curve.len(), 90);
    let summary = summarize(curve, 252.0);
    assert!(!summary.pnl.is_zero());
}
