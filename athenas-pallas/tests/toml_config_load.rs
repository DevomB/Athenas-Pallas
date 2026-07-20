use athenas_pallas::backtest::{BacktestConfig, DataFormat};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::types::Asset;
use rust_decimal::Decimal;

#[test]
fn loads_backtest_toml_fields() {
    let toml = r#"
[instrument]
exchange = "cme"
symbol = "ES"
asset_class = "future"
contract_multiplier = "50"
tick_size = "0.25"
lot_size = "1"
expiry = "2025-03"

[backtest]
data = "data/ES_1d.csv"
data_format = "ohlcv"
fee_bps = 10
slippage_bps = 5
half_spread_bps = 8
periods_per_year = 252.0

[[balances]]
asset = "USD"
amount = "100000"
"#;
    let dir = std::env::temp_dir().join("pallas_toml_test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("backtest.toml");
    std::fs::write(&path, toml).unwrap();

    let cfg = BacktestConfig::load_toml(&path).unwrap();

    assert_eq!(cfg.instrument.symbol, "ES");
    assert_eq!(cfg.asset_class, AssetClass::Future);
    assert_eq!(cfg.contract_multiplier, Some(Decimal::from(50u64)));
    assert_eq!(cfg.data_format, DataFormat::Ohlcv);
    assert_eq!(
        cfg.balances.get(&Asset("USD".into())),
        Some(&Decimal::from(100_000u64))
    );
}
