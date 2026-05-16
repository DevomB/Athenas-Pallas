//! Exchange instruments as compact row indices (cache-friendly hot state).

use crate::types::{Asset, InstrumentId};
use std::collections::HashMap;

/// Row index into per-instrument vectors in [`crate::state::GlobalState`].
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstrumentIndex(pub usize);

/// Static metadata for an instrument (base/quote assets).
#[derive(Clone, Debug)]
pub struct InstrumentMeta {
    /// Base asset (e.g. BTC).
    pub base: Asset,
    /// Quote asset (e.g. USDT).
    pub quote: Asset,
}

/// O(1) lookup from [`InstrumentId`] to dense index and metadata.
#[derive(Clone, Debug)]
pub struct InstrumentRegistry {
    ids: Vec<InstrumentId>,
    metas: Vec<InstrumentMeta>,
    by_id: HashMap<InstrumentId, InstrumentIndex>,
}

impl InstrumentRegistry {
    /// Build a registry from a map (sorted by id for deterministic iteration in tests).
    pub fn from_instruments(map: HashMap<InstrumentId, InstrumentMeta>) -> Self {
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

    /// Number of instruments.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// True if no instruments registered.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Dense index for `id`, if registered.
    pub fn index_of(&self, id: &InstrumentId) -> Option<InstrumentIndex> {
        self.by_id.get(id).copied()
    }

    /// Instrument id for a row.
    pub fn id(&self, ix: InstrumentIndex) -> Option<&InstrumentId> {
        self.ids.get(ix.0)
    }

    /// Metadata for a row.
    pub fn meta(&self, ix: InstrumentIndex) -> Option<&InstrumentMeta> {
        self.metas.get(ix.0)
    }

    /// Metadata by public id.
    pub fn meta_by_id(&self, id: &InstrumentId) -> Option<&InstrumentMeta> {
        let ix = self.index_of(id)?;
        self.meta(ix)
    }
}

/// Filter for instrument-scoped engine commands (extend as needed).
#[derive(Clone, Debug)]
pub enum InstrumentFilter {
    /// No restriction (all instruments).
    All,
    /// Single pair.
    One(InstrumentId),
}
