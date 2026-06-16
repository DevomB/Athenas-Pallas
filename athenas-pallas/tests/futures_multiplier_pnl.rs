use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, BuyAndHold, DataFormat};
use athenas_pallas::instrument::{AssetClass, InstrumentMeta};
use athenas_pallas::state::GlobalState;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

fn write_es_fixture(path: &PathBuf) {
    let mut f = std::fs::File::create(path).unwrap();
    writeln!(f, "Date,Open,High,Low,Close,Volume").unwrap();
    writeln!(f, "2024-01-02,5000,5010,4990,5000,100000").unwrap();
    writeln!(f, "2024-01-03,5000,5020,4995,5010,100000").unwrap();
}

#[test]
fn futures_pnl_scales_by_multiplier() {
    let dir = std::env::temp_dir().join("pallas_futures_pnl");
    let _ = std::fs::create_dir_all(&dir);
    let csv = dir.join("es.csv");
    write_es_fixture(&csv);

    let instrument = InstrumentId::new("cme", "ES");
    let mut balances = HashMap::new();
    balances.insert(Asset("USD".into()), Decimal::from(100_000u64));

    let spot_cfg = BacktestConfig {
        data_path: csv.clone(),
        data_format: DataFormat::Future,
        instrument: instrument.clone(),
        asset_class: AssetClass::Crypto,
        balances: balances.clone(),
        contract_multiplier: None,
        ..BacktestConfig::default()
    };

    let fut_cfg = BacktestConfig {
        asset_class: AssetClass::Future,
        contract_multiplier: Some(Decimal::from(50u64)),
        tick_size: Some(Decimal::new(25, 2)),
        ..spot_cfg.clone()
    };

    let mut spot_strategy = BuyAndHold::new(instrument.clone(), Decimal::ONE);
    let spot = BacktestRunner::run_with_strategy(&spot_cfg, &mut spot_strategy).unwrap();

    let mut fut_strategy = BuyAndHold::new(instrument.clone(), Decimal::ONE);
    let fut = BacktestRunner::run_with_strategy(&fut_cfg, &mut fut_strategy).unwrap();

    let spot_pnl: Decimal = spot.pnl.parse().unwrap();
    let fut_pnl: Decimal = fut.pnl.parse().unwrap();
    eprintln!("spot_pnl={spot_pnl} fut_pnl={fut_pnl}");
    assert!(fut_pnl.abs() > spot_pnl.abs() * Decimal::from(10u64));
}

#[test]
fn mark_equity_uses_multiplier() {
    let i = InstrumentId::new("cme", "ES");
    let mut map = HashMap::new();
    map.insert(
        i.clone(),
        InstrumentMeta::future(
            "ES",
            "USD",
            Decimal::from(50u64),
            Decimal::new(25, 2),
            Some(Decimal::ONE),
            None,
        ),
    );
    let mut bal = HashMap::new();
    bal.insert(Asset("USD".into()), Decimal::from(100_000u64));
    bal.insert(Asset("ES".into()), Decimal::ONE);
    let reg = athenas_pallas::instrument::InstrumentRegistry::from_instruments(map);
    let mut state = GlobalState::new(reg, bal);
    state.bar_close[0] = Some(Decimal::from(5000u64));
    state.l1[0] = Some((
        time::OffsetDateTime::now_utc(),
        Decimal::from(4999u64),
        Decimal::from(5001u64),
    ));
    let eq = state.mark_to_market_equity_ix(0).unwrap();
    assert!(eq > Decimal::from(300_000u64));
}
