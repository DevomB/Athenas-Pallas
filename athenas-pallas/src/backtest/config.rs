//! Backtest run configuration.
#![allow(missing_docs)]

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::instrument::{AssetClass, InstrumentMeta, OptionContractMeta, OptionKind};
use crate::types::{Asset, InstrumentId};

/// Extra instrument registered alongside the primary backtest symbol.
#[derive(Clone, Debug)]
pub struct ExtraInstrument {
    /// Instrument id.
    pub instrument: InstrumentId,
    /// Asset class.
    pub asset_class: AssetClass,
    /// Optional lot size.
    pub lot_size: Option<Decimal>,
    /// Optional tick size.
    pub tick_size: Option<Decimal>,
    /// Optional futures multiplier.
    pub contract_multiplier: Option<Decimal>,
    /// Optional expiry.
    pub expiry: Option<String>,
    /// Optional initial margin rate.
    pub margin_initial_rate: Option<Decimal>,
    /// Explicit option right.
    pub option_kind: Option<OptionKind>,
    /// Explicit option strike.
    pub option_strike: Option<Decimal>,
    /// Linked option underlying.
    pub option_underlying: Option<InstrumentId>,
    /// Historical CSV for this symbol (multi-instrument replay).
    pub data_path: Option<PathBuf>,
    /// CSV layout for `data_path`.
    pub data_format: Option<DataFormat>,
}

/// CSV layout for historical data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataFormat {
    #[default]
    Auto,
    Ohlcv,
    Fx,
    Jsonl,
}

impl DataFormat {
    /// Parse user-facing data-format aliases.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "ohlcv" => Ok(Self::Ohlcv),
            "fx" => Ok(Self::Fx),
            "jsonl" | "events" => Ok(Self::Jsonl),
            _ => Err(format!("unsupported data format `{s}`")),
        }
    }
}

/// Parse user-facing asset-class aliases.
pub fn parse_asset_class(s: &str) -> Result<AssetClass, String> {
    match s.to_ascii_lowercase().as_str() {
        "crypto" | "crypto_spot" | "spot" => Ok(AssetClass::Crypto),
        "equity" | "equities" => Ok(AssetClass::Equity),
        "forex" | "fx" => Ok(AssetClass::Forex),
        "future" | "futures" => Ok(AssetClass::Future),
        "option" | "options" => Ok(AssetClass::Option),
        "perpetual" | "perp" => Ok(AssetClass::Perpetual),
        "bond" | "bonds" => Ok(AssetClass::Bond),
        "hybrid" => Ok(AssetClass::Hybrid),
        _ => Err(format!("unsupported asset class `{s}`")),
    }
}

/// Parse an option right.
pub fn parse_option_kind(s: &str) -> Result<OptionKind, String> {
    match s.to_ascii_lowercase().as_str() {
        "call" => Ok(OptionKind::Call),
        "put" => Ok(OptionKind::Put),
        _ => Err(format!("unsupported option kind `{s}`; use call or put")),
    }
}

/// Parse user-facing data-format aliases.
pub fn parse_data_format(s: &str) -> Result<DataFormat, String> {
    DataFormat::parse(s)
}

/// Optional progress callback for CLI/integration consumers (`bar N` messages).
pub type ProgressHook = Arc<dyn Fn(&str) + Send + Sync>;

/// User-facing backtest settings.
#[derive(Clone)]
pub struct BacktestConfig {
    pub data_path: PathBuf,
    pub data_format: DataFormat,
    pub instrument: InstrumentId,
    pub asset_class: AssetClass,
    pub balances: HashMap<Asset, Decimal>,
    pub fee_bps: Decimal,
    pub slippage_bps: Decimal,
    pub half_spread_bps: Decimal,
    /// Quantity used by the built-in buy-and-hold strategy; one instrument lot when omitted.
    pub buy_and_hold_qty: Option<Decimal>,
    pub periods_per_year: f64,
    /// Declared bar interval (e.g. `30m`, `1d`) for Sharpe annualization when set.
    pub bar_interval: Option<String>,
    /// Session filter: `none`, `equity_rth`, `forex_245`.
    pub session_filter: Option<String>,
    /// When true, override `periods_per_year` from `bar_interval` or inferred bar spacing.
    pub auto_periods_per_year: bool,
    pub lot_size: Option<Decimal>,
    pub tick_size: Option<Decimal>,
    pub contract_multiplier: Option<Decimal>,
    pub expiry: Option<String>,
    pub option_kind: Option<OptionKind>,
    pub option_strike: Option<Decimal>,
    pub option_underlying: Option<InstrumentId>,
    pub record_equity_curve: bool,
    pub strategy_path: Option<PathBuf>,
    /// Arbitrary JSON-compatible parameters forwarded to external strategy initialization.
    pub strategy_parameters: HashMap<String, serde_json::Value>,
    pub python_exe: String,
    pub output_path: Option<PathBuf>,
    pub verbose: bool,
    pub on_progress: Option<ProgressHook>,
    /// Annualized risk-free rate for Sharpe/Sortino (e.g. `0.05`).
    pub risk_free_annual: f64,
    /// Max absolute position in base units for the primary instrument.
    pub max_position_abs: Option<Decimal>,
    /// Default initial margin rate when not set on instrument meta.
    pub margin_initial_rate: Option<Decimal>,
    /// Max daily loss in primary quote units (backtest risk).
    pub max_daily_loss_quote: Option<Decimal>,
    /// Additional instruments to register in the replay registry.
    pub extra_instruments: Vec<ExtraInstrument>,
    /// Explicit base asset when symbol parsing is insufficient.
    pub base_asset: Option<String>,
    /// Explicit quote asset when symbol parsing is insufficient.
    pub quote_asset: Option<String>,
}

impl std::fmt::Debug for BacktestConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BacktestConfig")
            .field("data_path", &self.data_path)
            .field("instrument", &self.instrument)
            .field("on_progress", &self.on_progress.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            data_path: PathBuf::new(),
            data_format: DataFormat::Auto,
            instrument: InstrumentId::new("test", "EXAMPLE"),
            asset_class: AssetClass::Equity,
            balances: HashMap::new(),
            base_asset: Some("EXAMPLE".into()),
            quote_asset: Some("USD".into()),
            fee_bps: Decimal::from(10u64),
            slippage_bps: Decimal::from(5u64),
            half_spread_bps: Decimal::from(5u64),
            buy_and_hold_qty: None,
            periods_per_year: 365.0,
            bar_interval: None,
            session_filter: None,
            auto_periods_per_year: true,
            lot_size: None,
            tick_size: None,
            contract_multiplier: None,
            expiry: None,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
            record_equity_curve: true,
            strategy_path: None,
            strategy_parameters: HashMap::new(),
            python_exe: "python".into(),
            output_path: None,
            verbose: false,
            on_progress: None,
            risk_free_annual: 0.0,
            max_position_abs: None,
            margin_initial_rate: None,
            max_daily_loss_quote: None,
            extra_instruments: Vec::new(),
        }
    }
}

impl BacktestConfig {
    /// Resolved base/quote for instrument metadata construction.
    pub fn resolved_base_quote(&self) -> (String, String) {
        if let (Some(base), Some(quote)) = (&self.base_asset, &self.quote_asset) {
            return (base.clone(), quote.clone());
        }
        parse_base_quote(&self.instrument.symbol, self.asset_class)
    }

    /// Default cash balance when none is supplied (10_000 in the resolved quote asset).
    pub fn default_balances(&self) -> HashMap<Asset, Decimal> {
        let (_, quote) = self.resolved_base_quote();
        let mut balances = HashMap::new();
        balances.insert(Asset::new(quote), Decimal::new(10_000, 0));
        balances
    }
}

/// Split `exchange:symbol`.
pub fn parse_instrument(s: &str) -> Result<InstrumentId, String> {
    let (ex, sym) = s
        .split_once(':')
        .ok_or_else(|| "instrument must be exchange:symbol".to_string())?;
    if ex.is_empty() || sym.is_empty() {
        return Err("empty exchange or symbol".into());
    }
    Ok(InstrumentId::new(ex, sym))
}

/// Infer base/quote from symbol string.
pub fn parse_base_quote(symbol: &str, class: AssetClass) -> (String, String) {
    match class {
        AssetClass::Forex if symbol.len() >= 6 => {
            (symbol[..3].to_string(), symbol[3..].to_string())
        }
        AssetClass::Equity => (symbol.to_string(), "USD".to_string()),
        AssetClass::Bond => (symbol.to_string(), "USD".to_string()),
        AssetClass::Option | AssetClass::Perpetual | AssetClass::Hybrid => {
            (symbol.to_string(), "USD".to_string())
        }
        _ => (symbol.to_string(), "USD".to_string()),
    }
}

/// Build registry metadata from primary backtest config.
pub fn instrument_meta_from_config(cfg: &BacktestConfig) -> InstrumentMeta {
    let (base, quote) = cfg.resolved_base_quote();
    build_instrument_meta(MetaFields {
        base,
        quote,
        asset_class: cfg.asset_class,
        lot_size: cfg.lot_size,
        tick_size: cfg.tick_size,
        contract_multiplier: cfg.contract_multiplier,
        expiry: cfg.expiry.clone(),
        margin_initial_rate: cfg.margin_initial_rate,
        option_kind: cfg.option_kind,
        option_strike: cfg.option_strike,
        option_underlying: cfg.option_underlying.clone(),
    })
}

/// Build metadata for an extra registered instrument.
pub fn instrument_meta_from_extra(extra: &ExtraInstrument) -> InstrumentMeta {
    let (base, quote) = parse_base_quote(&extra.instrument.symbol, extra.asset_class);
    build_instrument_meta(MetaFields {
        base,
        quote,
        asset_class: extra.asset_class,
        lot_size: extra.lot_size,
        tick_size: extra.tick_size,
        contract_multiplier: extra.contract_multiplier,
        expiry: extra.expiry.clone(),
        margin_initial_rate: extra.margin_initial_rate,
        option_kind: extra.option_kind,
        option_strike: extra.option_strike,
        option_underlying: extra.option_underlying.clone(),
    })
}

struct MetaFields {
    base: String,
    quote: String,
    asset_class: AssetClass,
    lot_size: Option<Decimal>,
    tick_size: Option<Decimal>,
    contract_multiplier: Option<Decimal>,
    expiry: Option<String>,
    margin_initial_rate: Option<Decimal>,
    option_kind: Option<OptionKind>,
    option_strike: Option<Decimal>,
    option_underlying: Option<InstrumentId>,
}

fn build_instrument_meta(fields: MetaFields) -> InstrumentMeta {
    let MetaFields {
        base,
        quote,
        asset_class,
        lot_size,
        tick_size,
        contract_multiplier,
        expiry,
        margin_initial_rate,
        option_kind,
        option_strike,
        option_underlying,
    } = fields;
    match asset_class {
        AssetClass::Future => {
            let mut meta = InstrumentMeta::future(
                base,
                quote,
                contract_multiplier.unwrap_or(Decimal::ONE),
                tick_size.unwrap_or(Decimal::new(25, 2)),
                lot_size,
                expiry,
            );
            meta.margin_initial_rate = margin_initial_rate;
            meta
        }
        AssetClass::Perpetual => InstrumentMeta::perpetual(
            base,
            quote,
            contract_multiplier,
            margin_initial_rate.or(Some(Decimal::new(1, 1))),
        ),
        AssetClass::Option => InstrumentMeta::option_meta(
            base,
            quote,
            OptionContractMeta {
                contract_multiplier: contract_multiplier.unwrap_or(Decimal::ONE),
                tick_size: tick_size.unwrap_or(Decimal::new(1, 2)),
                margin_initial_rate,
                expiry: expiry.unwrap_or_default(),
                kind: option_kind.unwrap_or(OptionKind::Call),
                strike: option_strike.unwrap_or(Decimal::ZERO),
                underlying: option_underlying.unwrap_or_else(|| InstrumentId::new("", "")),
            },
        ),
        AssetClass::Bond => InstrumentMeta::bond(
            base,
            quote,
            contract_multiplier.unwrap_or(Decimal::from(1000u64)),
            Decimal::new(5, 2),
            2,
            expiry.clone(),
        ),
        AssetClass::Forex => {
            let lot = lot_size.or(Some(Decimal::from(100_000u64)));
            InstrumentMeta {
                base: Asset::new(base),
                quote: Asset::new(quote),
                asset_class,
                lot_size: lot,
                contract_multiplier,
                tick_size: tick_size.or(Some(Decimal::new(1, 5))),
                expiry: expiry.clone(),
                margin_initial_rate,
                face_value: None,
                coupon_rate: None,
                coupon_payments_per_year: None,
                maturity: expiry,
                option_kind: None,
                option_strike: None,
                option_underlying: None,
            }
        }
        _ => InstrumentMeta {
            base: Asset::new(base),
            quote: Asset::new(quote),
            asset_class,
            lot_size,
            contract_multiplier,
            tick_size,
            expiry: expiry.clone(),
            margin_initial_rate,
            face_value: None,
            coupon_rate: None,
            coupon_payments_per_year: None,
            maturity: expiry,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
        },
    }
}

/// Reject incomplete option economics before replay or strategy initialization.
pub fn validate_instruments(
    config: &BacktestConfig,
    instruments: &HashMap<InstrumentId, InstrumentMeta>,
) -> crate::Result<()> {
    let mut sources = std::collections::HashSet::from([config.instrument.clone()]);
    sources.extend(
        config
            .extra_instruments
            .iter()
            .filter(|extra| extra.data_path.is_some())
            .map(|extra| extra.instrument.clone()),
    );
    for (instrument, meta) in instruments {
        if meta.asset_class != AssetClass::Option {
            continue;
        }
        if meta.option_kind.is_none()
            || meta
                .option_strike
                .is_none_or(|strike| strike <= Decimal::ZERO)
            || meta
                .contract_multiplier
                .is_none_or(|multiplier| multiplier <= Decimal::ZERO)
            || meta.expiry.as_deref().is_none_or(str::is_empty)
        {
            return Err(crate::Error::Invalid(format!(
                "option {instrument} requires option_kind, positive option_strike, contract_multiplier, and expiry"
            )));
        }
        let underlying = meta.option_underlying.as_ref().ok_or_else(|| {
            crate::Error::Invalid(format!("option {instrument} requires option_underlying"))
        })?;
        if underlying == instrument || !instruments.contains_key(underlying) {
            return Err(crate::Error::Invalid(format!(
                "option {instrument} underlying {underlying} is not a distinct registered instrument"
            )));
        }
        if !sources.contains(instrument) || !sources.contains(underlying) {
            return Err(crate::Error::Invalid(format!(
                "option {instrument} and underlying {underlying} both require replay data"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_facing_enums() {
        assert_eq!(parse_asset_class("perp"), Ok(AssetClass::Perpetual));
        assert_eq!(parse_asset_class("options"), Ok(AssetClass::Option));
        assert_eq!(parse_asset_class("futures"), Ok(AssetClass::Future));
        assert!(parse_data_format("future").is_err());
        assert!(parse_data_format("unknown").is_err());
        assert_eq!(parse_option_kind("put"), Ok(OptionKind::Put));
    }

    #[test]
    fn option_metadata_requires_registered_underlying() {
        let option = InstrumentId::new("test", "SPY_CALL");
        let underlying = InstrumentId::new("test", "SPY");
        let config = BacktestConfig {
            instrument: option.clone(),
            asset_class: AssetClass::Option,
            option_kind: Some(OptionKind::Call),
            option_strike: Some(Decimal::from(100u64)),
            option_underlying: Some(underlying.clone()),
            contract_multiplier: Some(Decimal::from(100u64)),
            expiry: Some("2025-01-17".into()),
            ..BacktestConfig::default()
        };
        let option_meta = instrument_meta_from_config(&config);
        let mut instruments = HashMap::from([(option, option_meta)]);
        assert!(validate_instruments(&config, &instruments).is_err());

        instruments.insert(underlying, InstrumentMeta::spot("SPY", "USD"));
        let mut config = config;
        config.extra_instruments.push(ExtraInstrument {
            instrument: InstrumentId::new("test", "SPY"),
            asset_class: AssetClass::Equity,
            lot_size: None,
            tick_size: None,
            contract_multiplier: None,
            expiry: None,
            margin_initial_rate: None,
            option_kind: None,
            option_strike: None,
            option_underlying: None,
            data_path: Some(PathBuf::from("spy.csv")),
            data_format: Some(DataFormat::Ohlcv),
        });
        validate_instruments(&config, &instruments).unwrap();
    }
}
