//! Dense indexed instruments (barter `IndexedInstruments`).

use crate::instrument::asset::{Asset, ExchangeId, Symbol};
use crate::instrument::config::InstrumentConfig;
use crate::instrument::kind::{InstrumentId, InstrumentKind, Underlying};
use crate::instrument::registry::{InstrumentMeta, LegacyInstrumentId};
use std::collections::HashMap;

/// Row index into per-instrument vectors.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstrumentIndex(pub usize);

/// One instrument with its dense index.
#[derive(Clone, Debug)]
pub struct IndexedInstrument {
    /// Dense index.
    pub index: InstrumentIndex,
    /// Identity.
    pub id: InstrumentId,
    /// Metadata.
    pub meta: InstrumentMeta,
}

/// Cache-friendly indexed instrument set.
#[derive(Clone, Debug)]
pub struct IndexedInstruments {
    instruments: Vec<IndexedInstrument>,
    by_key: HashMap<String, InstrumentIndex>,
    registry: crate::instrument::registry::InstrumentRegistry,
}

impl IndexedInstruments {
    /// Build from config rows (barter `SystemConfig.instruments`).
    pub fn new(configs: Vec<InstrumentConfig>) -> Self {
        let mut ids = Vec::with_capacity(configs.len());
        for cfg in configs {
            let underlying = Underlying {
                base: Asset::new(cfg.underlying.base.clone()),
                quote: Asset::new(cfg.underlying.quote.clone()),
            };
            let kind = parse_kind(&cfg.kind, &cfg);
            let id = InstrumentId::from_config(
                ExchangeId::new(cfg.exchange.clone()),
                Symbol::new(cfg.name_exchange.clone()),
                underlying.clone(),
                kind,
            );
            let meta = InstrumentMeta::spot(underlying.base.clone(), underlying.quote.clone());
            ids.push((id, meta));
        }
        ids.sort_by(|(a, _), (b, _)| a.key().cmp(&b.key()));
        let mut instruments = Vec::with_capacity(ids.len());
        let mut by_key = HashMap::with_capacity(ids.len());
        let mut legacy_map = HashMap::new();
        for (i, (id, meta)) in ids.into_iter().enumerate() {
            let ix = InstrumentIndex(i);
            by_key.insert(id.key(), ix);
            legacy_map.insert(legacy_instrument_id(&id), meta.clone());
            instruments.push(IndexedInstrument {
                index: ix,
                id: id.clone(),
                meta,
            });
        }
        let registry = crate::instrument::registry::InstrumentRegistry::from_instruments(legacy_map);
        Self {
            instruments,
            by_key,
            registry,
        }
    }

    /// Number of instruments.
    pub fn len(&self) -> usize {
        self.instruments.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.instruments.is_empty()
    }

    /// Lookup by key `exchange:symbol`.
    pub fn index_of_key(&self, key: &str) -> Option<InstrumentIndex> {
        self.by_key.get(key).copied()
    }

    /// Instrument at index.
    pub fn get(&self, ix: InstrumentIndex) -> Option<&IndexedInstrument> {
        self.instruments.get(ix.0)
    }

    /// Iterator over indexed instruments.
    pub fn iter(&self) -> impl Iterator<Item = &IndexedInstrument> {
        self.instruments.iter()
    }

    /// Legacy registry for engine hot path.
    pub fn registry(&self) -> &crate::instrument::registry::InstrumentRegistry {
        &self.registry
    }
}

fn parse_kind(kind: &str, cfg: &InstrumentConfig) -> InstrumentKind {
    match kind {
        "perpetual" => InstrumentKind::Perpetual,
        "future" => InstrumentKind::Future {
            contract: crate::instrument::kind::FutureContract {
                expiry: cfg
                    .expiry
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
            },
        },
        "option" => InstrumentKind::Option {
            contract: crate::instrument::kind::OptionContract {
                strike: cfg.strike.clone().unwrap_or_else(|| "0".into()),
                kind: cfg
                    .option_kind
                    .as_ref()
                    .and_then(|s| match s.as_str() {
                        "call" => Some(crate::instrument::kind::OptionKind::Call),
                        "put" => Some(crate::instrument::kind::OptionKind::Put),
                        _ => None,
                    })
                    .unwrap_or(crate::instrument::kind::OptionKind::Call),
                exercise: crate::instrument::kind::OptionExercise::European,
                expiry: cfg
                    .expiry
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
            },
        },
        _ => InstrumentKind::Spot,
    }
}

fn legacy_instrument_id(id: &InstrumentId) -> LegacyInstrumentId {
    LegacyInstrumentId {
        exchange: id.exchange.to_string(),
        symbol: id.name_exchange.to_string(),
    }
}
