//! Library API for loading config and running backtests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::config::{instrument_meta_from_config, instrument_meta_from_extra, BacktestConfig};
use super::cpp_build::build_cpp_strategy;
use super::report::BacktestReport;
use super::runner::BacktestRunner;
use super::strategy_resolver::{resolve_strategy_path, ResolvedStrategy};
use crate::strategy::ExternalStrategy;

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
    instruments.extend(
        cfg.extra_instruments
            .iter()
            .map(|extra| (extra.instrument.clone(), instrument_meta_from_extra(extra))),
    );
    let balances = if cfg.balances.is_empty() {
        cfg.default_balances()
    } else {
        cfg.balances.clone()
    };

    let mut ext = spawn_external_strategy(cfg, strategy_path)?;
    ext.handshake_with_context(
        cfg.instrument.clone(),
        &instruments,
        &balances,
        cfg.fee_bps,
        &cfg.strategy_parameters,
    )?;
    let report = BacktestRunner::run_with_strategy_with_cancel(cfg, &mut ext, cancel)?;
    ext.take_error()?;
    Ok(report)
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
    use crate::backtest::DataFormat;
    use crate::types::Asset;
    use rust_decimal::Decimal;
    use std::path::PathBuf;

    #[test]
    fn load_toml_example_fields() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("backtest.toml.example");
        let cfg = BacktestConfig::load_toml(&path).unwrap();
        assert_eq!(cfg.instrument.symbol, "EXAMPLE");
        assert_eq!(cfg.data_format, DataFormat::Yahoo);
        assert_eq!(cfg.base_asset.as_deref(), Some("EXAMPLE"));
        assert_eq!(cfg.quote_asset.as_deref(), Some("USD"));
        assert_eq!(cfg.buy_and_hold_qty, Some(Decimal::ONE));
        assert_eq!(cfg.strategy_parameters["fast_window"], 5);
        assert!(cfg.strategy_path.is_some());
    }

    #[test]
    fn default_balances_use_resolved_quote() {
        let cfg = BacktestConfig::default();
        let balances = cfg.default_balances();
        assert_eq!(
            balances.get(&Asset::new("USD")),
            Some(&Decimal::new(10_000, 0))
        );
    }
}
