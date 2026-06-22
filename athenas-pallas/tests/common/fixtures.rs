//! Shared instrument and balance fixtures for integration tests.
#![allow(dead_code)]

use athenas_pallas::instrument::InstrumentMeta;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

/// Path to a file under `tests/fixtures/data/`.
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/data")
        .join(name)
}

/// Primary neutral instrument: `test:EXAMPLE`.
pub fn test_instrument() -> InstrumentId {
    InstrumentId::new("test", "EXAMPLE")
}

/// Equity spot metadata for [`test_instrument`].
pub fn test_instrument_meta() -> InstrumentMeta {
    InstrumentMeta::spot("EXAMPLE", "USD")
}

/// Default USD cash balance for backtests.
pub fn test_balances() -> HashMap<Asset, Decimal> {
    let mut b = HashMap::new();
    b.insert(Asset::new("USD"), Decimal::new(10_000, 0));
    b
}

/// Crypto-shaped fixture instrument for BTCUSDT CSV data (`test:BTCUSDT`).
pub fn crypto_fixture_instrument() -> InstrumentId {
    InstrumentId::new("test", "BTCUSDT")
}

/// Spot metadata for [`crypto_fixture_instrument`].
pub fn crypto_fixture_meta() -> InstrumentMeta {
    InstrumentMeta::spot("BTC", "USDT")
}
