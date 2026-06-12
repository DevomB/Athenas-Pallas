//! Asset and exchange identifiers.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt;

/// Exchange identifier (e.g. `binance_spot`).
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExchangeId(#[serde(with = "smol_str_serde")] pub SmolStr);

mod smol_str_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use smol_str::SmolStr;

    pub fn serialize<S: Serializer>(v: &SmolStr, s: S) -> Result<S::Ok, S::Error> {
        v.as_str().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SmolStr, D::Error> {
        let s = String::deserialize(d)?;
        Ok(SmolStr::new(s))
    }
}

impl ExchangeId {
    /// New exchange id.
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for ExchangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Trading pair symbol on the exchange (e.g. BTCUSDT).
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Symbol(#[serde(with = "smol_str_serde")] pub SmolStr);

impl Symbol {
    /// New symbol.
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Asset code (e.g. btc, usdt).
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Asset(#[serde(with = "smol_str_serde")] pub SmolStr);

impl Asset {
    /// New asset.
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }
}

impl From<&str> for Asset {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Asset {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
