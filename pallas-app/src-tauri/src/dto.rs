use athenas_pallas::backtest::{BacktestConfig, DataFormat, ExtraInstrument};
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
pub struct ExtraInstrumentDto {
    pub exchange: String,
    pub symbol: String,
    pub asset_class: String,
    #[serde(default)]
    pub data_path: Option<String>,
    #[serde(default)]
    pub data_format: Option<String>,
    #[serde(default)]
    pub lot_size: Option<String>,
    #[serde(default)]
    pub tick_size: Option<String>,
    #[serde(default)]
    pub contract_multiplier: Option<String>,
    #[serde(default)]
    pub expiry: Option<String>,
    #[serde(default)]
    pub margin_initial_rate: Option<String>,
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
    #[serde(default)]
    pub bar_interval: Option<String>,
    #[serde(default)]
    pub session_filter: Option<String>,
    #[serde(default)]
    pub auto_periods_per_year: Option<bool>,
    #[serde(default)]
    pub risk_free_annual: Option<f64>,
    #[serde(default)]
    pub max_position_abs: Option<String>,
    #[serde(default)]
    pub max_daily_loss_quote: Option<String>,
    #[serde(default)]
    pub margin_initial_rate: Option<String>,
    pub lot_size: Option<String>,
    pub tick_size: Option<String>,
    pub contract_multiplier: Option<String>,
    pub expiry: Option<String>,
    pub record_equity_curve: bool,
    pub strategy_path: Option<String>,
    pub python_exe: String,
    pub output_path: Option<String>,
    pub balances: Vec<BalanceDto>,
    #[serde(default)]
    pub extra_instruments: Vec<ExtraInstrumentDto>,
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
            bar_interval: None,
            session_filter: None,
            auto_periods_per_year: Some(true),
            risk_free_annual: Some(0.0),
            max_position_abs: None,
            max_daily_loss_quote: None,
            margin_initial_rate: None,
            extra_instruments: vec![],
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
            bar_interval: cfg.bar_interval.clone(),
            session_filter: cfg.session_filter.clone(),
            auto_periods_per_year: Some(cfg.auto_periods_per_year),
            risk_free_annual: Some(cfg.risk_free_annual),
            max_position_abs: cfg.max_position_abs.map(|d| d.to_string()),
            max_daily_loss_quote: cfg.max_daily_loss_quote.map(|d| d.to_string()),
            margin_initial_rate: cfg.margin_initial_rate.map(|d| d.to_string()),
            extra_instruments: cfg
                .extra_instruments
                .iter()
                .map(|e| ExtraInstrumentDto {
                    exchange: e.instrument.exchange.clone(),
                    symbol: e.instrument.symbol.clone(),
                    asset_class: format_asset_class(e.asset_class),
                    data_path: e.data_path.as_ref().map(|p| p.display().to_string()),
                    data_format: e.data_format.map(format_data_format),
                    lot_size: e.lot_size.map(|d| d.to_string()),
                    tick_size: e.tick_size.map(|d| d.to_string()),
                    contract_multiplier: e.contract_multiplier.map(|d| d.to_string()),
                    expiry: e.expiry.clone(),
                    margin_initial_rate: e.margin_initial_rate.map(|d| d.to_string()),
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
        let mut extra_instruments = Vec::new();
        for e in &self.extra_instruments {
            extra_instruments.push(ExtraInstrument {
                instrument: InstrumentId::new(&e.exchange, &e.symbol),
                asset_class: parse_asset_class(&e.asset_class),
                lot_size: parse_decimal_opt(e.lot_size.as_deref())?,
                tick_size: parse_decimal_opt(e.tick_size.as_deref())?,
                contract_multiplier: parse_decimal_opt(e.contract_multiplier.as_deref())?,
                expiry: e.expiry.clone(),
                margin_initial_rate: parse_decimal_opt(e.margin_initial_rate.as_deref())?,
                data_path: e.data_path.as_deref().map(PathBuf::from),
                data_format: e.data_format.as_deref().map(parse_data_format),
            });
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
            bar_interval: self.bar_interval.clone(),
            session_filter: self.session_filter.clone(),
            auto_periods_per_year: self.auto_periods_per_year.unwrap_or(true),
            risk_free_annual: self.risk_free_annual.unwrap_or(0.0),
            max_position_abs: parse_decimal_opt(self.max_position_abs.as_deref())?,
            max_daily_loss_quote: parse_decimal_opt(self.max_daily_loss_quote.as_deref())?,
            margin_initial_rate: parse_decimal_opt(self.margin_initial_rate.as_deref())?,
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
            extra_instruments,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResampleRequest {
    pub input_path: String,
    pub target_interval: String,
    pub output_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeSourceDto {
    pub format: String,
    pub exchange: String,
    pub symbol: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeRequest {
    pub sources: Vec<MergeSourceDto>,
    pub output_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CsvPreviewDto {
    pub headers: Vec<String>,
    pub head_rows: Vec<Vec<String>>,
    pub tail_rows: Vec<Vec<String>>,
    pub total_rows: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaperSessionConfigDto {
    pub exchange: String,
    pub symbol: String,
    pub fee_bps: u64,
    pub slippage_bps: u64,
    pub starting_balance_asset: String,
    pub starting_balance_amount: String,
    pub strategy_path: Option<String>,
    pub python_exe: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveSessionConfigDto {
    pub exchange: String,
    pub symbol: String,
    pub fee_bps: u64,
    pub slippage_bps: u64,
    pub starting_balance_asset: String,
    pub starting_balance_amount: String,
    pub strategy_path: Option<String>,
    pub python_exe: String,
    pub use_testnet: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenOrderDto {
    pub id: String,
    pub instrument: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<String>,
    pub stop_price: Option<String>,
    pub remaining_qty: String,
    pub original_qty: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionDto {
    pub instrument: String,
    pub qty: String,
    pub mark_price: Option<String>,
    pub notional: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceSnapshotDto {
    pub asset: String,
    pub amount: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionsSnapshotDto {
    pub balances: Vec<BalanceSnapshotDto>,
    pub positions: Vec<PositionDto>,
    pub equity: String,
    pub mark_price: Option<String>,
    pub paused: bool,
    pub trading_enabled: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectorStatusDto {
    pub status: String,
    pub instrument: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradingStateDto {
    pub mode: String,
    pub instrument: String,
    pub paused: bool,
    pub trading_enabled: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillEventDto {
    pub ts: String,
    pub instrument: String,
    pub side: String,
    pub qty: String,
    pub price: String,
    pub fee: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CredentialsDto {
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepRequest {
    pub base_config_path: String,
    pub sweep_path: String,
    pub output_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplySweepRequest {
    pub base_config_path: String,
    pub sweep_path: String,
    pub row_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepResultRow {
    pub name: String,
    pub pnl: f64,
    pub sharpe: f64,
    pub sortino: f64,
    pub max_drawdown: f64,
    pub closed_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepResultDto {
    pub rows: Vec<SweepResultRow>,
    pub output_path: String,
}

fn parse_asset_class(s: &str) -> AssetClass {
    match s.to_lowercase().as_str() {
        "equity" => AssetClass::Equity,
        "forex" | "fx" => AssetClass::Forex,
        "future" | "futures" => AssetClass::Future,
        "option" => AssetClass::Option,
        "perpetual" | "perp" => AssetClass::Perpetual,
        "bond" => AssetClass::Bond,
        "hybrid" => AssetClass::Hybrid,
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
        AssetClass::Option => "option".into(),
        AssetClass::Perpetual => "perpetual".into(),
        AssetClass::Bond => "bond".into(),
        AssetClass::Hybrid => "hybrid".into(),
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
        Some(v) => v.parse::<Decimal>().map(Some).map_err(|e| e.to_string()),
    }
}
