mod common;

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;
#[test]
fn eurusd_fx_replay() {
    let csv = common::fixture("EURUSD_sample.csv");
    let mut balances = HashMap::new();
    balances.insert(Asset("USD".into()), Decimal::new(10_000, 0));
    let cfg = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Fx,
        instrument: InstrumentId::new("fx", "EURUSD"),
        asset_class: AssetClass::Forex,
        balances,
        ..BacktestConfig::default()
    };
    let report = BacktestRunner::run_buy_and_hold(&cfg).expect("run");
    assert!(!report.equity_curve.is_empty());
}
