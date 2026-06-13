use athenas_pallas::backtest::{BacktestConfig, DataFormat};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::types::{Asset, InstrumentId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceDto {
    pub asset: String,
    pub amount: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigDto {
    pub data_path: String,
    pub data_format: String,
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    pub fee_bps: u64,
    pub slippage_bps: u64,
    pub half_spread_bps: u64,
    pub periods_per_year: f64,
    pub lot_size: Option<String>,
    pub tick_size: Option<String>,
    pub contract_multiplier: Option<String>,
    pub expiry: Option<String>,
    pub record_equity_curve: bool,
    pub strategy_path: Option<String>,
    pub python_exe: String,
    pub output_path: Option<String>,
    pub balances: Vec<BalanceDto>,
}

impl Default for ConfigDto {
    fn default() -> Self {
        Self {
            data_path: "data/BTCUSDT_live.csv".into(),
            data_format: "ohlcv".into(),
            exchange: "binance".into(),
            symbol: "BTCUSDT".into(),
            asset_class: "crypto".into(),
            fee_bps: 10,
            slippage_bps: 5,
            half_spread_bps: 5,
            periods_per_year: 365.0,
            lot_size: None,
            tick_size: None,
            contract_multiplier: None,
            expiry: None,
            record_equity_curve: true,
            strategy_path: Some("trading/strategies/simple_sma/strategy.py".into()),
            python_exe: "python".into(),
            output_path: None,
            balances: vec![BalanceDto {
                asset: "USDT".into(),
                amount: "10000".into(),
            }],
        }
    }
}

impl ConfigDto {
    pub fn from_backtest_config(cfg: &BacktestConfig) -> Self {
        Self {
            data_path: cfg.data_path.display().to_string(),
            data_format: format_data_format(cfg.data_format),
            exchange: cfg.instrument.exchange.clone(),
            symbol: cfg.instrument.symbol.clone(),
            asset_class: format_asset_class(cfg.asset_class),
            fee_bps: decimal_to_u64(cfg.fee_bps),
            slippage_bps: decimal_to_u64(cfg.slippage_bps),
            half_spread_bps: decimal_to_u64(cfg.half_spread_bps),
            periods_per_year: cfg.periods_per_year,
            lot_size: cfg.lot_size.map(|d| d.to_string()),
            tick_size: cfg.tick_size.map(|d| d.to_string()),
            contract_multiplier: cfg.contract_multiplier.map(|d| d.to_string()),
            expiry: cfg.expiry.clone(),
            record_equity_curve: cfg.record_equity_curve,
            strategy_path: cfg.strategy_path.as_ref().map(|p| p.display().to_string()),
            python_exe: cfg.python_exe.clone(),
            output_path: cfg.output_path.as_ref().map(|p| p.display().to_string()),
            balances: cfg
                .balances
                .iter()
                .map(|(a, v)| BalanceDto {
                    asset: a.0.to_string(),
                    amount: v.to_string(),
                })
                .collect(),
        }
    }

    pub fn to_backtest_config(&self) -> Result<BacktestConfig, String> {
        let mut balances = HashMap::new();
        for row in &self.balances {
            let amount: Decimal = row
                .amount
                .parse()
                .map_err(|e| format!("balance {}: {e}", row.asset))?;
            balances.insert(Asset::new(&row.asset), amount);
        }
        Ok(BacktestConfig {
            data_path: PathBuf::from(&self.data_path),
            data_format: parse_data_format(&self.data_format),
            instrument: InstrumentId::new(&self.exchange, &self.symbol),
            asset_class: parse_asset_class(&self.asset_class),
            balances,
            fee_bps: Decimal::from(self.fee_bps),
            slippage_bps: Decimal::from(self.slippage_bps),
            half_spread_bps: Decimal::from(self.half_spread_bps),
            periods_per_year: self.periods_per_year,
            lot_size: parse_decimal_opt(self.lot_size.as_deref())?,
            tick_size: parse_decimal_opt(self.tick_size.as_deref())?,
            contract_multiplier: parse_decimal_opt(self.contract_multiplier.as_deref())?,
            expiry: self.expiry.clone(),
            record_equity_curve: self.record_equity_curve,
            strategy_path: self.strategy_path.as_deref().map(PathBuf::from),
            python_exe: self.python_exe.clone(),
            output_path: self.output_path.as_deref().map(PathBuf::from),
            verbose: false,
            on_progress: None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchRequest {
    pub provider: String,
    pub symbol: String,
    pub interval: String,
    pub days: u64,
    pub output_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillDto {
    pub ts: String,
    pub side: String,
    pub qty: String,
    pub price: String,
    pub fee: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunResultDto {
    pub report: athenas_pallas::backtest::BacktestReportDto,
    pub fills: Vec<FillDto>,
    pub full_report_json: String,
    pub equity_curve_skipped: bool,
    pub equity_curve_downsampled: bool,
}

fn parse_asset_class(s: &str) -> AssetClass {
    match s.to_lowercase().as_str() {
        "equity" => AssetClass::Equity,
        "forex" | "fx" => AssetClass::Forex,
        "future" | "futures" => AssetClass::Future,
        _ => AssetClass::Crypto,
    }
}

fn parse_data_format(s: &str) -> DataFormat {
    match s.to_lowercase().as_str() {
        "ohlcv" => DataFormat::Ohlcv,
        "yahoo" => DataFormat::Yahoo,
        "fx" => DataFormat::Fx,
        "future" | "futures" => DataFormat::Future,
        _ => DataFormat::Auto,
    }
}

fn format_asset_class(c: AssetClass) -> String {
    match c {
        AssetClass::Equity => "equity".into(),
        AssetClass::Forex => "forex".into(),
        AssetClass::Future => "future".into(),
        AssetClass::Crypto => "crypto".into(),
    }
}

fn format_data_format(f: DataFormat) -> String {
    match f {
        DataFormat::Ohlcv => "ohlcv".into(),
        DataFormat::Yahoo => "yahoo".into(),
        DataFormat::Fx => "fx".into(),
        DataFormat::Future => "future".into(),
        DataFormat::Auto => "auto".into(),
    }
}

fn decimal_to_u64(d: Decimal) -> u64 {
    d.mantissa().unsigned_abs() as u64 / 10u64.pow(d.scale())
}

fn parse_decimal_opt(s: Option<&str>) -> Result<Option<Decimal>, String> {
    match s {
        None => Ok(None),
        Some(v) => v
            .parse::<Decimal>()
            .map(Some)
            .map_err(|e| e.to_string()),
    }
}
