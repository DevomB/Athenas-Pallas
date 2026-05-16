//! Load a minimal `system_config.json` (barter-style) and build an [`athenas_pallas::instrument::InstrumentRegistry`].
//!
//! ```text
//! cargo run -p system_config
//! ```

use athenas_pallas::instrument::{InstrumentMeta, InstrumentRegistry};
use athenas_pallas::types::{Asset, InstrumentId};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize)]
struct SystemConfig {
    instruments: Vec<InstrumentRow>,
}

#[derive(Debug, Deserialize)]
struct InstrumentRow {
    exchange: String,
    symbol: String,
    base: String,
    quote: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("system_config.json");
    let raw = fs::read_to_string(&path)?;
    let cfg: SystemConfig = serde_json::from_str(&raw)?;
    let mut map = HashMap::new();
    for row in cfg.instruments {
        let id = InstrumentId::new(row.exchange, row.symbol);
        map.insert(
            id,
            InstrumentMeta {
                base: Asset(row.base),
                quote: Asset(row.quote),
            },
        );
    }
    let reg = InstrumentRegistry::from_instruments(map);
    println!("loaded {} instruments", reg.len());
    Ok(())
}
