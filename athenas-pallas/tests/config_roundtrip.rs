use athenas_pallas::{IndexedInstruments, SystemConfig};
use rust_decimal::Decimal;

#[test]
fn barter_system_config_deserializes() {
    let raw = include_str!("../../examples/system_config/system_config.json");
    let cfg: SystemConfig = serde_json::from_str(raw).expect("parse");
    assert!(cfg.risk_free_return > Decimal::ZERO || cfg.executions.len() > 0);
    let indexed = IndexedInstruments::new(cfg.instruments);
    assert!(!indexed.is_empty());
}
