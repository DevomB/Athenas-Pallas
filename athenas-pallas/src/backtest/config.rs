//! Backtest run configuration.
#![allow(missing_docs)]

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::instrument::AssetClass;
use crate::types::{Asset, InstrumentId};

/// CSV layout for historical data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataFormat {
    #[default]
    Auto,
    Ohlcv,
    Yahoo,
    Fx,
}

/// User-facing backtest settings.
#[derive(Clone, Debug)]
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
    pub strategy_path: Option<PathBuf>,
    pub python_exe: String,
    pub output_path: Option<PathBuf>,
    pub verbose: bool,
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
            periods_per_year: 252.0,
            strategy_path: None,
            python_exe: "python".into(),
            output_path: None,
            verbose: false,
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
        _ if symbol.ends_with("USDT") => {
            (symbol.trim_end_matches("USDT").to_string(), "USDT".to_string())
        }
        _ if symbol.ends_with("USD") => {
            (symbol.trim_end_matches("USD").to_string(), "USD".to_string())
        }
        _ => (symbol.to_string(), "USD".to_string()),
    }
}
