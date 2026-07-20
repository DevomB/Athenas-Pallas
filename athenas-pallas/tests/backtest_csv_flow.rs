//! End-to-end CSV replay with simulated fills.

mod common;

use athenas_pallas::backtest::BuyAndHold;
use athenas_pallas::dispatch_event_sync;
use athenas_pallas::events::Event;
use athenas_pallas::execution::{PaperConfig, PaperExecution};
use athenas_pallas::metrics::summarize;
use athenas_pallas::risk::{PauseCheck, RiskEngine};
use athenas_pallas::state::{GlobalState, InstrumentRegistry};
use athenas_pallas::types::{EquityPoint, ExchangeId, Symbol};
use athenas_pallas::{default_tick_size, BarSeries, BarSeriesSource, HistoricalSource};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use std::path::PathBuf;

fn sample_csv() -> PathBuf {
    common::fixture("BTCUSDT_1d.csv")
}

#[test]
fn csv_replay_buy_and_hold() {
    let instrument = common::crypto_fixture_instrument();
    let mut instruments = std::collections::HashMap::new();
    instruments.insert(instrument.clone(), common::crypto_fixture_meta());
    let mut balances = std::collections::HashMap::new();
    balances.insert(
        athenas_pallas::types::Asset("USDT".into()),
        Decimal::new(10_000, 0),
    );
    balances.insert(athenas_pallas::types::Asset("BTC".into()), Decimal::ZERO);

    let registry = InstrumentRegistry::from_instruments(instruments);
    let mut state = GlobalState::new(registry, balances);
    let qty = Decimal::from_f64(0.01).unwrap_or(Decimal::ZERO);
    let mut strategy = BuyAndHold::new(instrument.clone(), qty);
    let risk = RiskEngine::new(vec![Box::new(PauseCheck)]);
    let exec = PaperExecution::new(PaperConfig::default());

    let series = BarSeries::from_csv_path(&sample_csv(), default_tick_size()).expect("csv");
    let mut src = BarSeriesSource::new(series, ExchangeId("test".into()), Symbol("BTCUSDT".into()));

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
