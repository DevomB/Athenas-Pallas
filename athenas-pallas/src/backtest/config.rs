//! Backtest run configuration.
#![allow(missing_docs)]

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::instrument::{AssetClass, InstrumentMeta};
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
    Yahoo,
    Fx,
    Future,
}

/// Optional progress callback for GUI/CLI (`bar N` messages).
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
    pub record_equity_curve: bool,
    pub strategy_path: Option<PathBuf>,
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
            instrument: InstrumentId::new("binance", "BTCUSDT"),
            asset_class: AssetClass::Crypto,
            balances: HashMap::new(),
            fee_bps: Decimal::from(10u64),
            slippage_bps: Decimal::from(5u64),
            half_spread_bps: Decimal::from(5u64),
            periods_per_year: 365.0,
            bar_interval: None,
            session_filter: None,
            auto_periods_per_year: true,
            lot_size: None,
            tick_size: None,
            contract_multiplier: None,
            expiry: None,
            record_equity_curve: true,
            strategy_path: None,
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
        _ if symbol.ends_with("USDT") => (
            symbol.trim_end_matches("USDT").to_string(),
            "USDT".to_string(),
        ),
        _ if symbol.ends_with("USD") => (
            symbol.trim_end_matches("USD").to_string(),
            "USD".to_string(),
        ),
        _ => (symbol.to_string(), "USD".to_string()),
    }
}

/// Build registry metadata from primary backtest config.
pub fn instrument_meta_from_config(cfg: &BacktestConfig) -> InstrumentMeta {
    instrument_meta_from_fields(
        &cfg.instrument.symbol,
        cfg.asset_class,
        cfg.lot_size,
        cfg.tick_size,
        cfg.contract_multiplier,
        cfg.expiry.clone(),
        cfg.margin_initial_rate,
    )
}

/// Build metadata for an extra registered instrument.
pub fn instrument_meta_from_extra(extra: &ExtraInstrument) -> InstrumentMeta {
    instrument_meta_from_fields(
        &extra.instrument.symbol,
        extra.asset_class,
        extra.lot_size,
        extra.tick_size,
        extra.contract_multiplier,
        extra.expiry.clone(),
        extra.margin_initial_rate,
    )
}

fn instrument_meta_from_fields(
    symbol: &str,
    asset_class: AssetClass,
    lot_size: Option<Decimal>,
    tick_size: Option<Decimal>,
    contract_multiplier: Option<Decimal>,
    expiry: Option<String>,
    margin_initial_rate: Option<Decimal>,
) -> InstrumentMeta {
    let (base, quote) = parse_base_quote(symbol, asset_class);
    let mut meta = match asset_class {
        AssetClass::Future => InstrumentMeta::future(
            base,
            quote,
            contract_multiplier.unwrap_or(Decimal::ONE),
            tick_size.unwrap_or(Decimal::new(25, 2)),
            lot_size,
            expiry.clone(),
        ),
        AssetClass::Perpetual => InstrumentMeta::perpetual(
            base,
            quote,
            contract_multiplier,
            margin_initial_rate.or(Some(Decimal::new(1, 1))),
        ),
        AssetClass::Option => {
            // Until dedicated `strike` TOML field: use `tick_size` as strike price.
            let strike = tick_size.unwrap_or(Decimal::from(100u64));
            InstrumentMeta::option_meta(
                base,
                quote,
                contract_multiplier.unwrap_or(Decimal::ONE),
                Decimal::new(1, 2),
                margin_initial_rate,
                expiry.clone(),
                strike,
            )
        }
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
        },
    };
    if meta.margin_initial_rate.is_none() {
        meta.margin_initial_rate = margin_initial_rate;
    }
    meta
}
