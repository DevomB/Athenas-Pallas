mod common;

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;
#[test]
fn yahoo_aapl_buy_and_hold() {
    let csv = common::fixture("AAPL_1d.csv");
    let mut balances = HashMap::new();
    balances.insert(Asset("USD".into()), Decimal::new(10_000, 0));
    let cfg = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Yahoo,
        instrument: InstrumentId::new("nasdaq", "AAPL"),
        asset_class: AssetClass::Equity,
        balances,
        ..BacktestConfig::default()
    };
    let report = BacktestRunner::run_buy_and_hold(&cfg).expect("run");
    assert!(report.equity_curve.len() > 1);
}
