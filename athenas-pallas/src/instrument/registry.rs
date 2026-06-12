//! Dense instrument registry (engine hot path).

use crate::instrument::asset::Asset;
use crate::instrument::index::InstrumentIndex;
use std::collections::HashMap;

/// Broad asset class for sizing and risk defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssetClass {
    /// Crypto spot / perp.
    #[default]
    Crypto,
    /// Listed equity.
    Equity,
    /// FX pair.
    Forex,
    /// Dated future.
    Future,
}

/// Static metadata for an instrument.
#[derive(Clone, Debug)]
pub struct InstrumentMeta {
    /// Base asset.
    pub base: Asset,
    /// Quote asset.
    pub quote: Asset,
    /// Asset class.
    pub asset_class: AssetClass,
    /// Minimum order increment in base units.
    pub lot_size: Option<rust_decimal::Decimal>,
    /// Quote currency per one point of price move (futures).
    pub contract_multiplier: Option<rust_decimal::Decimal>,
    /// Minimum price increment.
    pub tick_size: Option<rust_decimal::Decimal>,
    /// Contract month (e.g. `2025-03`).
    pub expiry: Option<String>,
}

impl InstrumentMeta {
    /// Crypto-style spot pair.
    pub fn spot(base: impl Into<Asset>, quote: impl Into<Asset>) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            asset_class: AssetClass::Crypto,
            lot_size: None,
            contract_multiplier: None,
            tick_size: None,
            expiry: None,
        }
    }

    /// Listed future (qty = contracts).
    pub fn future(
        base: impl Into<Asset>,
        quote: impl Into<Asset>,
        contract_multiplier: rust_decimal::Decimal,
        tick_size: rust_decimal::Decimal,
        lot_size: Option<rust_decimal::Decimal>,
        expiry: Option<String>,
    ) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            asset_class: AssetClass::Future,
            lot_size,
            contract_multiplier: Some(contract_multiplier),
            tick_size: Some(tick_size),
            expiry,
        }
    }
}

/// Legacy instrument key for engine state vectors.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct LegacyInstrumentId {
    /// Exchange.
    pub exchange: String,
    /// Symbol.
    pub symbol: String,
}

impl std::fmt::Display for LegacyInstrumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.exchange, self.symbol)
    }
}

impl LegacyInstrumentId {
    /// Construct from exchange and symbol strings.
    pub fn new(exchange: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            exchange: exchange.into(),
            symbol: symbol.into(),
        }
    }
}

/// O(1) lookup from legacy id to dense index.
#[derive(Clone, Debug)]
pub struct InstrumentRegistry {
    ids: Vec<LegacyInstrumentId>,
    metas: Vec<InstrumentMeta>,
    by_id: HashMap<LegacyInstrumentId, InstrumentIndex>,
}

impl InstrumentRegistry {
    /// Build from a map (sorted for determinism).
    pub fn from_instruments(map: HashMap<LegacyInstrumentId, InstrumentMeta>) -> Self {
        let mut pairs: Vec<_> = map.into_iter().collect();
        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut ids = Vec::with_capacity(pairs.len());
        let mut metas = Vec::with_capacity(pairs.len());
        let mut by_id = HashMap::with_capacity(pairs.len());
        for (i, (id, meta)) in pairs.into_iter().enumerate() {
            let ix = InstrumentIndex(i);
            by_id.insert(id.clone(), ix);
            ids.push(id);
            metas.push(meta);
        }
        Self {
            ids,
            metas,
            by_id,
        }
    }

    /// Count.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Empty check.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Index lookup.
    pub fn index_of(&self, id: &LegacyInstrumentId) -> Option<InstrumentIndex> {
        self.by_id.get(id).copied()
    }

    /// Id at index.
    pub fn id(&self, ix: InstrumentIndex) -> Option<&LegacyInstrumentId> {
        self.ids.get(ix.0)
    }

    /// Meta at index.
    pub fn meta(&self, ix: InstrumentIndex) -> Option<&InstrumentMeta> {
        self.metas.get(ix.0)
    }

    /// Meta by id.
    pub fn meta_by_id(&self, id: &LegacyInstrumentId) -> Option<&InstrumentMeta> {
        let ix = self.index_of(id)?;
        self.meta(ix)
    }
}
