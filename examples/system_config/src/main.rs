//! Load barter-style `system_config.json` and build [`IndexedInstruments`].

use athenas_pallas::{IndexedInstruments, SystemConfig};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/system_config.json");
    let raw = fs::read_to_string(path)?;
    let cfg: SystemConfig = serde_json::from_str(&raw)?;
    let indexed = IndexedInstruments::new(cfg.instruments);
    println!(
        "instruments={} executions={} risk_free_return={}",
        indexed.len(),
        cfg.executions.len(),
        cfg.risk_free_return
    );
    for ex in &cfg.executions {
        println!(
            "  mock {} latency_ms={} fees_percent={}",
            ex.mocked_exchange, ex.latency_ms, ex.fees_percent
        );
    }
    Ok(())
}
