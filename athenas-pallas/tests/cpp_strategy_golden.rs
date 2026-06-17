//! C++ SMA backtest matches the same golden metrics as Python.

mod common;

use athenas_pallas::backtest::{build_cpp_strategy, BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::strategy::ExternalStrategy;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

fn sample_csv() -> PathBuf {
    common::fixture("BTCUSDT_1d.csv")
}

fn cpp_strategy_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("trading")
        .join("simple_sma_cpp")
}

#[test]
#[ignore = "requires cmake"]
fn cpp_sma_matches_golden() {
    let dir = cpp_strategy_dir();
    if !dir.join("CMakeLists.txt").is_file() {
        if std::env::var("CI").is_ok() {
            panic!("C++ strategy not found at {}", dir.display());
        }
        eprintln!("skip: C++ strategy not found");
        return;
    }

    let golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/golden_sma_results.json")).unwrap();

    let binary = build_cpp_strategy(&dir).expect("cmake build");
    let instrument = InstrumentId::new("binance", "BTCUSDT");
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));

    let cfg = BacktestConfig {
        data_path: sample_csv(),
        data_format: DataFormat::Ohlcv,
        instrument: instrument.clone(),
        asset_class: AssetClass::Crypto,
        balances: balances.clone(),
        fee_bps: Decimal::from(10u64),
        slippage_bps: Decimal::from(5u64),
        periods_per_year: 252.0,
        python_exe: "python".into(),
        ..BacktestConfig::default()
    };

    let mut ext = ExternalStrategy::spawn_binary(&binary).expect("spawn");
    ext.handshake(
        instrument,
        &athenas_pallas::instrument::InstrumentMeta::spot("BTC", "USDT"),
        &balances,
        cfg.fee_bps,
    )
    .expect("handshake");

    let report = BacktestRunner::run_with_strategy(&cfg, &mut ext).expect("run");
    ext.take_error().expect("protocol");

    assert_eq!(report.pnl, golden["pnl"].as_str().unwrap());
    assert_eq!(report.fill_count, golden["fill_count"].as_u64().unwrap());
    assert_eq!(
        report.equity_curve.len(),
        golden["equity_curve_len"].as_u64().unwrap() as usize
    );
}
