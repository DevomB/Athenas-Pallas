//! Library API for loading config and running backtests (CLI and GUI).

use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::config::{instrument_meta_from_config, BacktestConfig, DataFormat, ExtraInstrument};
use super::cpp_build::build_cpp_strategy;
use super::runner::{BacktestReport, BacktestRunner};
use crate::instrument::AssetClass;
use crate::strategy::ExternalStrategy;
use crate::types::{Asset, EquityPoint};

/// JSON-friendly report for GUI / IPC (f64 metrics, no Decimal string parsing in JS).
#[derive(Clone, Debug, Serialize)]
pub struct EquityPointDto {
    /// Unix timestamp in milliseconds.
    pub ts_unix_ms: i64,
    /// Mark-to-market equity in quote currency.
    pub equity_f64: f64,
}

/// Downsampled, chart-ready backtest output.
#[derive(Clone, Debug, Serialize)]
pub struct BacktestReportDto {
    /// Net PnL in quote currency.
    pub pnl: f64,
    /// PnL as fraction of starting equity.
    pub pnl_pct: f64,
    /// Peak-to-trough drawdown (0..1).
    pub max_drawdown: f64,
    /// Annualized Sharpe ratio.
    pub sharpe: f64,
    /// Annualized Sortino ratio.
    pub sortino: f64,
    /// Number of fills.
    pub fill_count: u64,
    /// Wall-clock runtime in milliseconds.
    pub wall_time_ms: u64,
    /// Downsampled equity curve for charting.
    pub equity_curve: Vec<EquityPointDto>,
    /// Fraction of closed round-trips with positive PnL.
    pub win_rate: f64,
    /// Gross profit / gross loss.
    pub profit_factor: f64,
    /// Closed round-trip count.
    pub closed_trades: usize,
}

impl BacktestConfig {
    /// Load settings from a TOML file into a new config (defaults for omitted fields).
    pub fn load_toml(path: &Path) -> crate::Result<Self> {
        let mut cfg = Self::default();
        cfg.apply_toml(path)?;
        Ok(cfg)
    }

    /// Merge TOML file values into this config.
    pub fn apply_toml(&mut self, path: &Path) -> crate::Result<()> {
        let text = std::fs::read_to_string(path).map_err(crate::Error::Io)?;
        let table: toml::Table =
            toml::from_str(&text).map_err(|e| crate::Error::Invalid(e.to_string()))?;
        let base_dir = path.parent();
        apply_table(self, &table, base_dir)
    }
}

/// Run buy-and-hold backtest.
pub fn run_backtest(cfg: &BacktestConfig) -> crate::Result<BacktestReport> {
    run_backtest_with_cancel(cfg, None)
}

/// Run buy-and-hold with optional cooperative cancel (checked every 64 bars).
pub fn run_backtest_with_cancel(
    cfg: &BacktestConfig,
    cancel: Option<Arc<AtomicBool>>,
) -> crate::Result<BacktestReport> {
    BacktestRunner::run_buy_and_hold_with_cancel(cfg, cancel)
}

/// Run an external Python or C++ strategy subprocess.
pub fn run_external_backtest(
    cfg: &BacktestConfig,
    strategy_path: &Path,
) -> crate::Result<BacktestReport> {
    run_external_backtest_with_cancel(cfg, strategy_path, None)
}

/// Run external strategy with optional cancel flag.
pub fn run_external_backtest_with_cancel(
    cfg: &BacktestConfig,
    strategy_path: &Path,
    cancel: Option<Arc<AtomicBool>>,
) -> crate::Result<BacktestReport> {
    let meta = instrument_meta_from_config(cfg);
    let mut instruments = HashMap::new();
    instruments.insert(cfg.instrument.clone(), meta.clone());
    let balances = if cfg.balances.is_empty() {
        let mut b = HashMap::new();
        b.insert(Asset::new("USDT"), Decimal::new(10_000, 0));
        b
    } else {
        cfg.balances.clone()
    };

    let mut ext = spawn_external_strategy(cfg, strategy_path)?;
    ext.handshake(cfg.instrument.clone(), &meta, &balances, cfg.fee_bps)?;
    let report = BacktestRunner::run_with_strategy_with_cancel(cfg, &mut ext, cancel)?;
    ext.take_error()?;
    Ok(report)
}

/// Build chart-ready DTO from a full report (equity downsampled).
pub fn report_to_dto(report: &BacktestReport, max_chart_points: usize) -> BacktestReportDto {
    BacktestReportDto {
        pnl: decimal_str_to_f64(&report.pnl),
        pnl_pct: decimal_str_to_f64(&report.pnl_pct),
        max_drawdown: report.max_drawdown,
        sharpe: report.sharpe,
        sortino: report.sortino,
        fill_count: report.fill_count,
        wall_time_ms: report.wall_time_ms,
        equity_curve: downsample_equity(&report.equity_curve, max_chart_points),
        win_rate: report.win_rate,
        profit_factor: report.profit_factor,
        closed_trades: report.closed_trades,
    }
}

/// Reduce equity curve to at most `max_points`, preserving first and last samples.
pub fn downsample_equity(curve: &[EquityPoint], max_points: usize) -> Vec<EquityPointDto> {
    if curve.is_empty() {
        return Vec::new();
    }
    if max_points == 0 {
        return Vec::new();
    }
    if curve.len() <= max_points {
        return curve.iter().map(equity_point_to_dto).collect();
    }
    let last_idx = curve.len() - 1;
    let mut out = Vec::with_capacity(max_points);
    out.push(equity_point_to_dto(&curve[0]));
    if max_points == 1 {
        return out;
    }
    if max_points == 2 {
        out.push(equity_point_to_dto(&curve[last_idx]));
        return out;
    }
    let inner_slots = max_points - 2;
    let step = (last_idx as f64) / (inner_slots as f64 + 1.0);
    for i in 1..=inner_slots {
        let idx = (step * i as f64).round() as usize;
        let idx = idx.min(last_idx);
        out.push(equity_point_to_dto(&curve[idx]));
    }
    out.push(equity_point_to_dto(&curve[last_idx]));
    out
}

fn equity_point_to_dto(p: &EquityPoint) -> EquityPointDto {
    EquityPointDto {
        ts_unix_ms: p.ts.unix_timestamp() * 1000 + (p.ts.nanosecond() / 1_000_000) as i64,
        equity_f64: decimal_to_f64(p.equity_quote),
    }
}

fn decimal_to_f64(d: Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.to_f64().unwrap_or(0.0)
}

fn decimal_str_to_f64(s: &str) -> f64 {
    s.parse::<Decimal>().map(decimal_to_f64).unwrap_or(0.0)
}

fn parse_asset_class(s: &str) -> AssetClass {
    match s.to_lowercase().as_str() {
        "equity" => AssetClass::Equity,
        "forex" | "fx" => AssetClass::Forex,
        "future" | "futures" => AssetClass::Future,
        "option" | "options" => AssetClass::Option,
        "perpetual" | "perp" => AssetClass::Perpetual,
        "bond" | "bonds" => AssetClass::Bond,
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

fn parse_decimal_opt(s: &str) -> Option<Decimal> {
    s.parse().ok()
}

fn resolve_config_path(base_dir: Option<&Path>, p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        return path;
    }
    base_dir.map(|b| b.join(&path)).unwrap_or(path)
}

fn apply_table(
    cfg: &mut BacktestConfig,
    table: &toml::Table,
    base_dir: Option<&Path>,
) -> crate::Result<()> {
    if let Some(inst) = table.get("instrument").and_then(|v| v.as_table()) {
        if let (Some(ex), Some(sym)) = (
            inst.get("exchange").and_then(|v| v.as_str()),
            inst.get("symbol").and_then(|v| v.as_str()),
        ) {
            cfg.instrument = crate::types::InstrumentId::new(ex, sym);
        }
        if let Some(ac) = inst.get("asset_class").and_then(|v| v.as_str()) {
            cfg.asset_class = parse_asset_class(ac);
        }
        if let Some(v) = inst.get("lot_size").and_then(|v| v.as_str()) {
            cfg.lot_size = parse_decimal_opt(v);
        }
        if let Some(v) = inst.get("tick_size").and_then(|v| v.as_str()) {
            cfg.tick_size = parse_decimal_opt(v);
        }
        if let Some(v) = inst.get("contract_multiplier").and_then(|v| v.as_str()) {
            cfg.contract_multiplier = parse_decimal_opt(v);
        }
        if let Some(v) = inst.get("expiry").and_then(|v| v.as_str()) {
            cfg.expiry = Some(v.to_string());
        }
    }

    if let Some(bt) = table.get("backtest").and_then(|v| v.as_table()) {
        if let Some(p) = bt.get("data").and_then(|v| v.as_str()) {
            cfg.data_path = resolve_config_path(base_dir, p);
        }
        if let Some(f) = bt.get("data_format").and_then(|v| v.as_str()) {
            cfg.data_format = parse_data_format(f);
        }
        if let Some(fee) = bt.get("fee_bps").and_then(|v| v.as_integer()) {
            cfg.fee_bps = Decimal::from(fee as u64);
        }
        if let Some(slip) = bt.get("slippage_bps").and_then(|v| v.as_integer()) {
            cfg.slippage_bps = Decimal::from(slip as u64);
        }
        if let Some(hs) = bt.get("half_spread_bps").and_then(|v| v.as_integer()) {
            cfg.half_spread_bps = Decimal::from(hs as u64);
        }
        if let Some(py) = bt.get("periods_per_year").and_then(|v| v.as_float()) {
            cfg.periods_per_year = py;
            cfg.auto_periods_per_year = false;
        }
        if let Some(iv) = bt.get("bar_interval").and_then(|v| v.as_str()) {
            cfg.bar_interval = Some(iv.to_string());
        }
        if let Some(sf) = bt.get("session_filter").and_then(|v| v.as_str()) {
            cfg.session_filter = Some(sf.to_string());
        }
        if let Some(v) = bt.get("auto_periods_per_year").and_then(|v| v.as_bool()) {
            cfg.auto_periods_per_year = v;
        }
        if let Some(p) = bt.get("output").and_then(|v| v.as_str()) {
            cfg.output_path = Some(resolve_config_path(base_dir, p));
        }
        if let Some(p) = bt.get("strategy").and_then(|v| v.as_str()) {
            cfg.strategy_path = Some(resolve_config_path(base_dir, p));
        }
        if let Some(p) = bt.get("python").and_then(|v| v.as_str()) {
            cfg.python_exe = p.to_string();
        }
        if let Some(v) = bt.get("record_equity_curve").and_then(|v| v.as_bool()) {
            cfg.record_equity_curve = v;
        }
        if let Some(v) = bt.get("risk_free_annual").and_then(|v| v.as_float()) {
            cfg.risk_free_annual = v;
        }
        if let Some(v) = bt.get("max_position_abs").and_then(|v| v.as_str()) {
            cfg.max_position_abs = parse_decimal_opt(v);
        }
        if let Some(v) = bt.get("max_daily_loss_quote").and_then(|v| v.as_str()) {
            cfg.max_daily_loss_quote = parse_decimal_opt(v);
        }
        if let Some(v) = bt.get("margin_initial_rate").and_then(|v| v.as_str()) {
            cfg.margin_initial_rate = parse_decimal_opt(v);
        }
    }

    if let Some(fee) = table.get("fee_bps").and_then(|v| v.as_integer()) {
        cfg.fee_bps = Decimal::from(fee as u64);
    }
    if let Some(slip) = table.get("slippage_bps").and_then(|v| v.as_integer()) {
        cfg.slippage_bps = Decimal::from(slip as u64);
    }

    if let Some(rows) = table.get("balances").and_then(|v| v.as_array()) {
        cfg.balances.clear();
        for row in rows {
            let Some(tbl) = row.as_table() else {
                continue;
            };
            let Some(asset) = tbl.get("asset").and_then(|v| v.as_str()) else {
                continue;
            };
            let amount = tbl
                .get("amount")
                .and_then(|v| v.as_str())
                .and_then(parse_decimal_opt)
                .ok_or_else(|| {
                    crate::Error::Invalid(format!("invalid balance amount for {asset}"))
                })?;
            cfg.balances.insert(Asset::new(asset), amount);
        }
    }

    if let Some(rows) = table.get("instruments").and_then(|v| v.as_array()) {
        cfg.extra_instruments.clear();
        for row in rows {
            let Some(tbl) = row.as_table() else {
                continue;
            };
            let Some(ex) = tbl.get("exchange").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(sym) = tbl.get("symbol").and_then(|v| v.as_str()) else {
                continue;
            };
            let ac = tbl
                .get("asset_class")
                .and_then(|v| v.as_str())
                .map(parse_asset_class)
                .unwrap_or(AssetClass::Crypto);
            let mut extra = ExtraInstrument {
                instrument: crate::types::InstrumentId::new(ex, sym),
                asset_class: ac,
                lot_size: tbl
                    .get("lot_size")
                    .and_then(|v| v.as_str())
                    .and_then(parse_decimal_opt),
                tick_size: tbl
                    .get("tick_size")
                    .and_then(|v| v.as_str())
                    .and_then(parse_decimal_opt),
                contract_multiplier: tbl
                    .get("contract_multiplier")
                    .and_then(|v| v.as_str())
                    .and_then(parse_decimal_opt),
                expiry: tbl.get("expiry").and_then(|v| v.as_str()).map(String::from),
                margin_initial_rate: tbl
                    .get("margin_initial_rate")
                    .and_then(|v| v.as_str())
                    .and_then(parse_decimal_opt),
                data_path: tbl
                    .get("data")
                    .and_then(|v| v.as_str())
                    .map(|p| resolve_config_path(base_dir, p)),
                data_format: tbl
                    .get("data_format")
                    .and_then(|v| v.as_str())
                    .map(parse_data_format),
            };
            if extra.data_path.is_none() {
                if let Some(p) = tbl.get("data_path").and_then(|v| v.as_str()) {
                    extra.data_path = Some(resolve_config_path(base_dir, p));
                }
            }
            cfg.extra_instruments.push(extra);
        }
    }

    Ok(())
}

fn spawn_external_strategy(
    cfg: &BacktestConfig,
    strategy_path: &Path,
) -> crate::Result<ExternalStrategy> {
    if strategy_path.is_dir() && strategy_path.join("CMakeLists.txt").is_file() {
        let binary = build_cpp_strategy(strategy_path)?;
        return ExternalStrategy::spawn_binary(&binary);
    }
    let script = resolve_strategy_path(strategy_path)?;
    if script.extension().and_then(|e| e.to_str()) == Some("py") {
        ExternalStrategy::spawn_python(&script, &cfg.python_exe)
    } else {
        ExternalStrategy::spawn_binary(&script)
    }
}

fn resolve_strategy_path(path: &Path) -> crate::Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if path.is_dir() {
        if path.join("CMakeLists.txt").is_file() {
            return Ok(path.to_path_buf());
        }
        let py = path.join("strategy.py");
        if py.is_file() {
            return Ok(py);
        }
        let main_py = path.join("main.py");
        if main_py.is_file() {
            return Ok(main_py);
        }
    }
    Err(crate::Error::Invalid(format!(
        "no strategy script at {}",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use time::macros::datetime;

    #[test]
    fn dto_roundtrip_json() {
        let report = BacktestReport {
            pnl: "100.5".into(),
            pnl_pct: "0.01".into(),
            max_drawdown: 0.05,
            sharpe: 1.2,
            sortino: 1.5,
            fill_count: 3,
            equity_curve: vec![EquityPoint {
                ts: datetime!(2024-01-01 00:00:00 UTC),
                equity_quote: Decimal::new(10_000, 0),
            }],
            fills: vec![],
            wall_time_ms: 42,
            win_rate: 0.0,
            profit_factor: 0.0,
            closed_trades: 0,
        };
        let dto = report_to_dto(&report, 2000);
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"pnl\":100.5"));
        assert!(json.contains("\"equity_f64\":10000"));
    }

    #[test]
    fn downsample_preserves_endpoints() {
        let curve: Vec<EquityPoint> = (0..100_000)
            .map(|i| EquityPoint {
                ts: datetime!(2024-01-01 00:00:00 UTC) + time::Duration::seconds(i),
                equity_quote: Decimal::from(i),
            })
            .collect();
        let out = downsample_equity(&curve, 2000);
        assert!(out.len() <= 2000);
        assert_eq!(out.first().unwrap().equity_f64, 0.0);
        assert_eq!(out.last().unwrap().equity_f64, 99_999.0);
    }

    #[test]
    fn load_toml_example_fields() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("backtest.toml.example");
        let cfg = BacktestConfig::load_toml(&path).unwrap();
        assert_eq!(cfg.instrument.symbol, "BTCUSDT");
        assert_eq!(cfg.data_format, DataFormat::Ohlcv);
        assert!(cfg.strategy_path.is_some());
    }
}
