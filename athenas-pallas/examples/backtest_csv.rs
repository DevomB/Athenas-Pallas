//! Backtest the canonical OHLCV fixture through the same runner used by the CLI.
//!
//! ```text
//! cargo run -p athenas-pallas --example backtest_csv
//! ```

use athenas_pallas::backtest::{run_backtest, BacktestConfig, DataFormat};
use rust_decimal::Decimal;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BacktestConfig {
        data_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/data/EXAMPLE_1d.csv"),
        data_format: DataFormat::Ohlcv,
        buy_and_hold_qty: Some(Decimal::from(10)),
        ..BacktestConfig::default()
    };
    let report = run_backtest(&config)?;

    println!("PnL: {}", report.pnl);
    println!("PnL %: {}", report.pnl_pct);
    println!("Max drawdown: {}", report.max_drawdown);
    println!("Sharpe: {}", report.sharpe);
    println!("Sortino: {}", report.sortino);
    Ok(())
}
