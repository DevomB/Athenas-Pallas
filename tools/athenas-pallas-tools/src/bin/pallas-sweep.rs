//! Parameter sweep over a TOML backtest config grid.

use athenas_pallas::backtest::{run_backtest, BacktestConfig};
use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "pallas-sweep", about = "Grid search over backtest parameters")]
struct Args {
    /// Base TOML config (same schema as pallas-backtest).
    #[arg(long)]
    config: PathBuf,
    /// Sweep definition TOML with `[[sweep]]` rows overriding base fields.
    #[arg(long)]
    sweep: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
struct SweepFile {
    sweep: Vec<SweepRow>,
}

#[derive(Debug, Deserialize)]
struct SweepRow {
    name: String,
    #[serde(flatten)]
    overrides: toml::Table,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let base_txt = std::fs::read_to_string(&args.config)?;
    let base: toml::Table = toml::from_str(&base_txt)?;
    let sweep_file: SweepFile = toml::from_str(&std::fs::read_to_string(&args.sweep)?)?;

    let mut wtr = csv::Writer::from_path(&args.output)?;
    wtr.write_record([
        "name",
        "pnl",
        "pnl_pct",
        "sharpe",
        "sortino",
        "max_drawdown",
        "closed_trades",
        "win_rate",
        "profit_factor",
    ])?;

    for row in &sweep_file.sweep {
        let mut table = base.clone();
        for (k, v) in &row.overrides {
            table.insert(k.clone(), v.clone());
        }
        let merged = toml::to_string(&table)?;
        let tmp = std::env::temp_dir().join(format!("pallas-sweep-{}.toml", row.name));
        std::fs::write(&tmp, merged)?;
        let cfg = BacktestConfig::load_toml(&tmp)?;
        let report = run_backtest(&cfg)?;
        let pnl = report.pnl.clone();
        wtr.write_record([
            row.name.clone(),
            report.pnl,
            report.pnl_pct,
            format!("{:.4}", report.sharpe),
            format!("{:.4}", report.sortino),
            format!("{:.4}", report.max_drawdown),
            report.closed_trades.to_string(),
            format!("{:.4}", report.win_rate),
            format!("{:.4}", report.profit_factor),
        ])?;
        println!("{} pnl={}", row.name, pnl);
    }
    wtr.flush()?;
    println!("wrote sweep results to {}", args.output.display());
    Ok(())
}
