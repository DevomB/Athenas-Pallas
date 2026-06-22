//! JSONL fixture replays through the sync replay path.

mod common;

use athenas_pallas::backtest::{read_events_jsonl, replay_events_serial};
use athenas_pallas::execution::{PaperConfig, SimGateway};
use athenas_pallas::instrument::InstrumentRegistry;
use athenas_pallas::risk::{PauseCheck, RiskPipeline};
use athenas_pallas::state::GlobalState;
use athenas_pallas::strategy::NoopStrategy;
use athenas_pallas::types::Asset;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

fn jsonl_fixture() -> PathBuf {
    common::fixture("events_sample.jsonl")
}

#[test]
fn jsonl_replay_yields_ten_events() {
    let path = jsonl_fixture();
    if !path.is_file() {
        eprintln!("skip: {}", path.display());
        return;
    }
    let file = File::open(&path).expect("open");
    let events = read_events_jsonl(file).expect("read");
    assert_eq!(events.len(), 10);

    let inst = events[0].instrument().cloned().expect("instrument on bar");
    let mut instruments = HashMap::new();
    instruments.insert(
        inst.clone(),
        athenas_pallas::instrument::InstrumentMeta::spot("EXAMPLE", "USD"),
    );
    let mut balances = HashMap::new();
    balances.insert(Asset("USD".into()), Decimal::new(10_000, 0));
    let state = GlobalState::new(InstrumentRegistry::from_instruments(instruments), balances);
    let strategy = NoopStrategy;
    let risk = RiskPipeline::new(vec![Box::new(PauseCheck)]);
    let exec = SimGateway::new(PaperConfig::default());
    let final_state = replay_events_serial(state, strategy, &risk, &exec, events).expect("replay");
    assert_eq!(final_state.fill_count, 0);
}
