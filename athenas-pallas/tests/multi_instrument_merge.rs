//! Multi-instrument merged replay registers both symbols.

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat, ExtraInstrument};
use athenas_pallas::instrument::AssetClass;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn merged_extra_instrument_updates_registry() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let mut balances = HashMap::new();
    balances.insert(
        athenas_pallas::types::Asset("USD".into()),
        Decimal::from(10_000u64),
    );

    let cfg = BacktestConfig {
        data_path: base.join("EXAMPLE_1d.csv"),
        data_format: DataFormat::Ohlcv,
        instrument: athenas_pallas::types::InstrumentId::new("test", "EXAMPLE"),
        asset_class: AssetClass::Equity,
        base_asset: Some("EXAMPLE".into()),
        quote_asset: Some("USD".into()),
        balances,
        extra_instruments: vec![ExtraInstrument {
            instrument: athenas_pallas::types::InstrumentId::new("test", "BTCUSDT"),
            asset_class: AssetClass::Crypto,
            lot_size: None,
            tick_size: None,
            contract_multiplier: None,
            expiry: None,
            margin_initial_rate: None,
            data_path: Some(base.join("BTCUSDT_1d.pbar")),
            data_format: Some(DataFormat::Auto),
        }],
        ..BacktestConfig::default()
    };

    let report = BacktestRunner::run_buy_and_hold(&cfg).unwrap();
    assert!(report.fill_count >= 1);
    assert!(!report.equity_curve.is_empty());
    assert_eq!(report.data.sources[1].format, "ohlcv");
}
