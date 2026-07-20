//! External strategy IPC failure modes.

mod common;

use athenas_pallas::backtest::{
    run_external_backtest, BacktestConfig, BacktestRunner, DataFormat, ExtraInstrument,
};
use athenas_pallas::instrument::{AssetClass, InstrumentMeta};
use athenas_pallas::strategy::ExternalStrategy;
use athenas_pallas::types::Asset;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn sample_csv() -> PathBuf {
    common::fixture("BTCUSDT_1d.csv")
}

fn cfg() -> BacktestConfig {
    let instrument = common::crypto_fixture_instrument();
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    BacktestConfig {
        data_path: sample_csv(),
        data_format: DataFormat::Ohlcv,
        instrument,
        asset_class: AssetClass::Crypto,
        base_asset: Some("BTC".into()),
        quote_asset: Some("USDT".into()),
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
        "instrument": {"exchange": "test", "symbol": "BTCUSDT"},
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

#[test]
fn versioned_protocol_reports_fills_and_flattens_on_finish() {
    let script = write_temp_script(
        "pallas_protocol_v2.py",
        r#"
import json, sys
init = json.loads(sys.stdin.readline())
assert init["protocol_version"] == 2
assert init["parameters"]["window"] == 20
assert {item["symbol"] for item in init["instruments"]} == {"BTCUSDT", "AAPL"}
sys.stdout.write(json.dumps({"msg": "ready", "capabilities": ["finish"]}) + "\n")
sys.stdout.flush()
placed = False
saw_fill = False
while True:
    message = json.loads(sys.stdin.readline())
    if message["msg"] == "shutdown":
        break
    if message["msg"] == "event":
        ctx = message["ctx"]
        assert "pending_orders" in ctx and len(ctx["instruments"]) == 2
        if ctx["fills"]:
            saw_fill = saw_fill or ctx["fills"][0].get("client_order_id") == "entry-1"
        intents = []
        if not placed:
            placed = True
            intents = [{
                "instrument": {"exchange": "test", "symbol": "BTCUSDT"},
                "side": "Buy",
                "order_type": "Market",
                "qty": "0.01",
                "client_order_id": "entry-1"
            }]
        sys.stdout.write(json.dumps({"msg": "intents", "seq": message["seq"], "intents": intents}) + "\n")
        sys.stdout.flush()
    elif message["msg"] == "finish":
        assert saw_fill
        sys.stdout.write(json.dumps({
            "msg": "intents", "seq": message["seq"], "intents": [], "flatten": True
        }) + "\n")
        sys.stdout.flush()
"#,
    );

    let mut config = cfg();
    config
        .strategy_parameters
        .insert("window".into(), 20.into());
    config.extra_instruments.push(ExtraInstrument {
        instrument: athenas_pallas::types::InstrumentId::new("yahoo", "AAPL"),
        asset_class: AssetClass::Equity,
        lot_size: Some(Decimal::ONE),
        tick_size: Some(Decimal::new(1, 2)),
        contract_multiplier: None,
        expiry: None,
        margin_initial_rate: None,
        data_path: None,
        data_format: None,
    });

    let report = run_external_backtest(&config, &script).expect("versioned protocol run");

    assert_eq!(report.fill_count, 2);
    assert_eq!(
        report
            .final_positions
            .iter()
            .find(|position| position.instrument == config.instrument)
            .unwrap()
            .qty,
        "0.00"
    );
}

#[test]
fn external_strategy_can_inspect_and_cancel_pending_by_client_id() {
    let script = write_temp_script(
        "pallas_cancel_pending.py",
        r#"
import json, sys
json.loads(sys.stdin.readline())
sys.stdout.write(json.dumps({"msg": "ready"}) + "\n")
sys.stdout.flush()
placed = False
canceled = False
while True:
    message = json.loads(sys.stdin.readline())
    if message["msg"] == "shutdown":
        break
    if message["msg"] != "event":
        continue
    response = {"msg": "intents", "seq": message["seq"], "intents": []}
    pending = message["ctx"].get("pending_orders", [])
    if pending and not canceled:
        assert pending[0]["client_order_id"] == "resting-1"
        response["cancel_client_order_ids"] = ["resting-1"]
        canceled = True
    elif not placed:
        response["intents"] = [{
            "instrument": {"exchange": "test", "symbol": "BTCUSDT"},
            "side": "Buy",
            "order_type": "Limit",
            "qty": "0.01",
            "price": "1",
            "client_order_id": "resting-1"
        }]
        placed = True
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()
"#,
    );

    let config = cfg();
    let meta = InstrumentMeta::spot("BTC", "USDT");
    let mut strategy = ExternalStrategy::spawn_python(&script, "python").expect("spawn");
    strategy
        .handshake(
            config.instrument.clone(),
            &meta,
            &config.balances,
            config.fee_bps,
        )
        .expect("handshake");

    let report = BacktestRunner::run_with_strategy(&config, &mut strategy).expect("run");
    strategy.take_error().expect("protocol");

    assert_eq!(report.fill_count, 0);
    assert!(report.pending_orders.is_empty());
}
