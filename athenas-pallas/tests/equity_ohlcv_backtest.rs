//! Canonical OHLCV replay for an equity instrument.

mod common;

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;

#[test]
fn equity_buy_and_hold() {
    let csv = common::fixture("EXAMPLE_1d.csv");
    let balances = HashMap::from([(Asset::new("USD"), Decimal::new(10_000, 0))]);
    let cfg = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Ohlcv,
        instrument: InstrumentId::new("test", "EXAMPLE"),
        asset_class: AssetClass::Equity,
        balances,
        ..BacktestConfig::default()
    };
    let report = BacktestRunner::run_buy_and_hold(&cfg).expect("run");
    assert!(report.equity_curve.len() > 1);
    assert_eq!(report.parameters.periods_per_year, 252.0);
}
