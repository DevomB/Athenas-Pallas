//! External strategy IPC failure modes.

mod common;

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::instrument::{AssetClass, InstrumentMeta};
use athenas_pallas::strategy::ExternalStrategy;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn sample_csv() -> PathBuf {
    common::fixture("BTCUSDT_1d.csv")
}

fn cfg() -> BacktestConfig {
    let instrument = InstrumentId::new("binance", "BTCUSDT");
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    BacktestConfig {
        data_path: sample_csv(),
        data_format: DataFormat::Ohlcv,
        instrument,
        asset_class: AssetClass::Crypto,
        balances,
        fee_bps: Decimal::from(10u64),
        slippage_bps: Decimal::from(5u64),
        periods_per_year: 252.0,
        python_exe: "python".into(),
        ..BacktestConfig::default()
    }
}

fn write_temp_script(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    fs::write(&path, body).expect("write script");
    path
}

#[test]
fn python_stub_returns_intent_on_first_bar() {
    let script = write_temp_script(
        "pallas_stub_strategy.py",
        r#"
import json, sys
msg = json.loads(sys.stdin.readline())
assert msg["msg"] == "init"
sys.stdout.write(json.dumps({"msg": "ready"}) + "\n")
sys.stdout.flush()
while True:
    line = sys.stdin.readline()
    if not line:
        break
    m = json.loads(line)
    if m.get("msg") == "shutdown":
        break
    if m.get("msg") != "event":
        continue
    intents = [{
        "instrument": {"exchange": "binance", "symbol": "BTCUSDT"},
        "side": "Buy",
        "order_type": "Market",
        "qty": "0.01"
    }]
    sys.stdout.write(json.dumps({"msg": "intents", "seq": m["seq"], "intents": intents}) + "\n")
    sys.stdout.flush()
"#,
    );

    let c = cfg();
    let meta = InstrumentMeta::spot("BTC", "USDT");
    let mut ext = ExternalStrategy::spawn_python(&script, "python").expect("spawn");
    ext.handshake(c.instrument.clone(), &meta, &c.balances, c.fee_bps)
        .expect("handshake");
    let report = BacktestRunner::run_with_strategy(&c, &mut ext).expect("run");
    ext.take_error().expect("protocol");
    assert!(report.fill_count >= 1);
}

#[test]
fn child_exit_on_init_is_error() {
    let script = write_temp_script("pallas_crash.py", "import sys; sys.exit(1)");
    let mut ext = ExternalStrategy::spawn_python(&script, "python").expect("spawn");
    let c = cfg();
    let meta = InstrumentMeta::spot("BTC", "USDT");
    let handshake = ext.handshake(c.instrument.clone(), &meta, &c.balances, c.fee_bps);
    assert!(handshake.is_err());
}

#[test]
fn no_ready_within_timeout_is_error() {
    let script = write_temp_script(
        "pallas_slow.py",
        r#"
import time
time.sleep(30)
"#,
    );
    let mut ext = ExternalStrategy::spawn_python(&script, "python").expect("spawn");
    let c = cfg();
    let meta = InstrumentMeta::spot("BTC", "USDT");
    let handshake = ext.handshake(c.instrument.clone(), &meta, &c.balances, c.fee_bps);
    assert!(handshake.is_err());
    let msg = handshake.unwrap_err().to_string();
    assert!(msg.contains("timeout") || msg.contains("exited"));
}
