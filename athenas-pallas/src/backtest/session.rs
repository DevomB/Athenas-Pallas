//! Library API for loading config and running backtests.

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::config::{instrument_meta_from_config, BacktestConfig, DataFormat, ExtraInstrument};
use super::cpp_build::build_cpp_strategy;
use super::runner::{BacktestReport, BacktestRunner};
use super::strategy_resolver::{resolve_strategy_path, ResolvedStrategy};
use crate::instrument::AssetClass;
use crate::strategy::ExternalStrategy;
use crate::types::Asset;

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
    match resolve_strategy_path(strategy_path)? {
        ResolvedStrategy::CmakeCpp(dir) => {
            let binary = build_cpp_strategy(&dir)?;
            ExternalStrategy::spawn_binary(&binary)
        }
        ResolvedStrategy::Python(script) => {
            ExternalStrategy::spawn_python(&script, &cfg.python_exe)
        }
        ResolvedStrategy::Binary(binary) => ExternalStrategy::spawn_binary(&binary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
