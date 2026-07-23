//! Dense instrument registry (engine hot path).

use crate::instrument::asset::Asset;
use crate::instrument::index::InstrumentIndex;
use crate::instrument::pricing::OptionKind;
use rustc_hash::FxHashMap;
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
    /// Listed option.
    Option,
    /// Perpetual swap.
    Perpetual,
    /// Fixed-income bond.
    Bond,
    /// Multi-leg or mixed structure.
    Hybrid,
}

/// Typed economics required for one listed European option.
#[derive(Clone, Debug)]
pub struct OptionContractMeta {
    /// Quote value of one price point.
    pub contract_multiplier: rust_decimal::Decimal,
    /// Minimum premium increment.
    pub tick_size: rust_decimal::Decimal,
    /// Optional short-option margin rate.
    pub margin_initial_rate: Option<rust_decimal::Decimal>,
    /// Expiration date.
    pub expiry: String,
    /// Call or put.
    pub kind: OptionKind,
    /// Strike in quote units.
    pub strike: rust_decimal::Decimal,
    /// Linked underlying instrument.
    pub underlying: InstrumentId,
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
    /// Initial margin as fraction of notional (e.g. `0.1` = 10%).
    pub margin_initial_rate: Option<rust_decimal::Decimal>,
    /// Bond face / par value in quote currency.
    pub face_value: Option<rust_decimal::Decimal>,
    /// Annual coupon rate as decimal (e.g. `0.05` = 5%).
    pub coupon_rate: Option<rust_decimal::Decimal>,
    /// Coupon payments per calendar year.
    pub coupon_payments_per_year: Option<u32>,
    /// Bond maturity date string.
    pub maturity: Option<String>,
    /// Explicit option right.
    pub option_kind: Option<OptionKind>,
    /// Option strike in quote units.
    pub option_strike: Option<rust_decimal::Decimal>,
    /// Linked underlying instrument for option settlement.
    pub option_underlying: Option<InstrumentId>,
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
            margin_initial_rate: None,
            face_value: None,
            coupon_rate: None,
            coupon_payments_per_year: None,
            maturity: None,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
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
            margin_initial_rate: None,
            face_value: None,
            coupon_rate: None,
            coupon_payments_per_year: None,
            maturity: None,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
        }
    }

    /// Perpetual swap (qty = contracts or base units).
    pub fn perpetual(
        base: impl Into<Asset>,
        quote: impl Into<Asset>,
        contract_multiplier: Option<rust_decimal::Decimal>,
        margin_initial_rate: Option<rust_decimal::Decimal>,
    ) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            asset_class: AssetClass::Perpetual,
            lot_size: None,
            contract_multiplier,
            tick_size: None,
            expiry: None,
            margin_initial_rate,
            face_value: None,
            coupon_rate: None,
            coupon_payments_per_year: None,
            maturity: None,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
        }
    }

    /// Fixed-income bond.
    pub fn bond(
        base: impl Into<Asset>,
        quote: impl Into<Asset>,
        face_value: rust_decimal::Decimal,
        coupon_rate: rust_decimal::Decimal,
        coupon_payments_per_year: u32,
        maturity: Option<String>,
    ) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            asset_class: AssetClass::Bond,
            lot_size: None,
            contract_multiplier: None,
            tick_size: None,
            expiry: None,
            margin_initial_rate: None,
            face_value: Some(face_value),
            coupon_rate: Some(coupon_rate),
            coupon_payments_per_year: Some(coupon_payments_per_year),
            maturity,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
        }
    }

    /// Listed European option contract metadata.
    pub fn option_meta(
        base: impl Into<Asset>,
        quote: impl Into<Asset>,
        contract: OptionContractMeta,
    ) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            asset_class: AssetClass::Option,
            lot_size: None,
            contract_multiplier: Some(contract.contract_multiplier),
            tick_size: Some(contract.tick_size),
            expiry: Some(contract.expiry),
            margin_initial_rate: contract.margin_initial_rate,
            face_value: None,
            coupon_rate: None,
            coupon_payments_per_year: None,
            maturity: None,
            option_kind: Some(contract.kind),
            option_strike: Some(contract.strike),
            option_underlying: Some(contract.underlying),
        }
    }
}

/// Exchange and symbol key used by engine state vectors.
#[derive(
    Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct InstrumentId {
    /// Exchange.
    pub exchange: String,
    /// Symbol.
    pub symbol: String,
}

impl std::fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.exchange, self.symbol)
    }
}

impl InstrumentId {
    /// Construct from exchange and symbol strings.
    pub fn new(exchange: impl Into<String>, symbol: impl Into<String>) -> Self {
        Self {
            exchange: exchange.into(),
            symbol: symbol.into(),
        }
    }
}

/// O(1) lookup from instrument id to dense index.
#[derive(Clone, Debug)]
pub struct InstrumentRegistry {
    ids: Vec<InstrumentId>,
    metas: Vec<InstrumentMeta>,
    // Internal, trusted, small-key map: a faster non-DoS-resistant hasher is fine here.
    by_id: FxHashMap<InstrumentId, InstrumentIndex>,
}

impl InstrumentRegistry {
    /// Build from a map (sorted for determinism).
    pub fn from_instruments(map: HashMap<InstrumentId, InstrumentMeta>) -> Self {
        let mut pairs: Vec<_> = map.into_iter().collect();
        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut ids = Vec::with_capacity(pairs.len());
        let mut metas = Vec::with_capacity(pairs.len());
        let mut by_id = FxHashMap::with_capacity_and_hasher(pairs.len(), rustc_hash::FxBuildHasher);
        for (i, (id, meta)) in pairs.into_iter().enumerate() {
            let ix = InstrumentIndex(i);
            by_id.insert(id.clone(), ix);
            ids.push(id);
            metas.push(meta);
        }
        Self { ids, metas, by_id }
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
    pub fn index_of(&self, id: &InstrumentId) -> Option<InstrumentIndex> {
        self.by_id.get(id).copied()
    }

    /// Id at index.
    pub fn id(&self, ix: InstrumentIndex) -> Option<&InstrumentId> {
        self.ids.get(ix.0)
    }

    /// Dense id/meta rows in index order.
    pub fn iter(&self) -> impl Iterator<Item = (InstrumentIndex, &InstrumentId, &InstrumentMeta)> {
        self.ids
            .iter()
            .zip(&self.metas)
            .enumerate()
            .map(|(ix, (id, meta))| (InstrumentIndex(ix), id, meta))
    }

    /// Meta at index.
    pub fn meta(&self, ix: InstrumentIndex) -> Option<&InstrumentMeta> {
        self.metas.get(ix.0)
    }

    /// Meta by id.
    pub fn meta_by_id(&self, id: &InstrumentId) -> Option<&InstrumentMeta> {
        let ix = self.index_of(id)?;
        self.meta(ix)
    }
}
