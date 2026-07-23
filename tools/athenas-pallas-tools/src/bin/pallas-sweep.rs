//! Parameter sweep and strategy catalog runner.

use athenas_pallas::backtest::{
    run_backtest, run_external_backtest, BacktestConfig, BacktestReport,
};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

#[derive(Parser, Debug)]
#[command(
    name = "pallas-sweep",
    about = "Run a parameter grid or strategy catalog"
)]
struct Args {
    /// Base TOML config (same schema as pallas-backtest).
    #[arg(long)]
    config: PathBuf,
    /// TOML with `[[sweep]]` rows overriding base fields.
    #[arg(long, required_unless_present = "catalog", conflicts_with = "catalog")]
    sweep: Option<PathBuf>,
    /// TOML with `[[strategy]]` rows naming strategy paths and parameters.
    #[arg(long, required_unless_present = "sweep", conflicts_with = "sweep")]
    catalog: Option<PathBuf>,
    /// Maximum backtests to run concurrently.
    #[arg(long, default_value_t = 1)]
    jobs: usize,
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

#[derive(Debug, Deserialize)]
struct CatalogFile {
    strategy: Vec<CatalogRow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogRow {
    name: String,
    path: PathBuf,
    #[serde(default)]
    parameters: HashMap<String, serde_json::Value>,
}

struct Job {
    index: usize,
    name: String,
    config: BacktestConfig,
}

struct JobResult {
    index: usize,
    name: String,
    report: Result<BacktestReport, String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.jobs == 0 {
        return Err("--jobs must be at least 1".into());
    }
    let base_text = std::fs::read_to_string(&args.config)?;
    let base_table: toml::Table = toml::from_str(&base_text)?;
    let base_dir = args.config.parent();
    let jobs = if let Some(path) = &args.sweep {
        build_sweep_jobs(&base_table, base_dir, path)?
    } else {
        build_catalog_jobs(&base_text, base_dir, args.catalog.as_deref().unwrap())?
    };
    if jobs.is_empty() {
        return Err("sweep/catalog has no rows".into());
    }

    let results = run_jobs(&jobs, args.jobs);
    write_results(&args.output, results)?;
    println!("wrote results to {}", args.output.display());
    Ok(())
}

fn build_sweep_jobs(
    base: &toml::Table,
    base_dir: Option<&Path>,
    path: &Path,
) -> Result<Vec<Job>, Box<dyn std::error::Error>> {
    let sweep_file: SweepFile = toml::from_str(&std::fs::read_to_string(path)?)?;
    sweep_file
        .sweep
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut table = base.clone();
            for (key, value) in row.overrides {
                table.insert(key, value);
            }
            let text = toml::to_string(&table)?;
            let config = BacktestConfig::load_toml_text(&text, base_dir)?;
            Ok(Job {
                index,
                name: row.name,
                config,
            })
        })
        .collect()
}

fn build_catalog_jobs(
    base_text: &str,
    base_dir: Option<&Path>,
    path: &Path,
) -> Result<Vec<Job>, Box<dyn std::error::Error>> {
    let catalog: CatalogFile = toml::from_str(&std::fs::read_to_string(path)?)?;
    let catalog_dir = path.parent().unwrap_or_else(|| Path::new("."));
    catalog
        .strategy
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut config = BacktestConfig::load_toml_text(base_text, base_dir)?;
            config.strategy_path = Some(if row.path.is_absolute() {
                row.path
            } else {
                catalog_dir.join(row.path)
            });
            config.strategy_parameters.extend(row.parameters);
            Ok(Job {
                index,
                name: row.name,
                config,
            })
        })
        .collect()
}

fn run_jobs(jobs: &[Job], requested_workers: usize) -> Vec<JobResult> {
    let worker_count = requested_workers.min(jobs.len());
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let sender = sender.clone();
            let next = &next;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = jobs.get(index) else {
                    break;
                };
                let report = run_configured(&job.config).map_err(|error| error.to_string());
                if sender
                    .send(JobResult {
                        index: job.index,
                        name: job.name.clone(),
                        report,
                    })
                    .is_err()
                {
                    break;
                }
            });
        }
    });
    drop(sender);

    let mut results: Vec<_> = receiver.into_iter().collect();
    results.sort_by_key(|result| result.index);
    results
}

fn run_configured(config: &BacktestConfig) -> athenas_pallas::Result<BacktestReport> {
    match config.strategy_path.as_deref() {
        Some(path) => run_external_backtest(config, path),
        None => run_backtest(config),
    }
}

fn write_results(output: &Path, results: Vec<JobResult>) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = csv::Writer::from_path(output)?;
    writer.write_record([
        "name",
        "status",
        "error",
        "pnl",
        "pnl_pct",
        "sharpe",
        "sortino",
        "max_drawdown",
        "closed_trades",
        "win_rate",
        "profit_factor",
    ])?;

    let mut failures = Vec::new();
    for result in results {
        match result.report {
            Ok(report) => {
                println!("{} pnl={}", result.name, report.pnl);
                writer.write_record([
                    result.name,
                    "ok".to_owned(),
                    String::new(),
                    report.pnl,
                    report.pnl_pct,
                    format!("{:.4}", report.sharpe),
                    format!("{:.4}", report.sortino),
                    format!("{:.4}", report.max_drawdown),
                    report.closed_trades.to_string(),
                    format!("{:.4}", report.win_rate),
                    format!("{:.4}", report.profit_factor),
                ])?;
            }
            Err(error) => {
                eprintln!("{} failed: {}", result.name, error);
                writer.write_record([
                    result.name.clone(),
                    "error".to_owned(),
                    error.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ])?;
                failures.push(format!("{}: {}", result.name, error));
            }
        }
    }
    writer.flush()?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} backtest(s) failed: {}",
            failures.len(),
            failures.join("; ")
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    fn temp_file(name: &str, contents: &str) -> PathBuf {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("pallas-sweep-{}-{id}-{name}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn catalog_builds_external_strategy_jobs_and_merges_parameters() {
        let catalog = temp_file(
            "catalog.toml",
            r#"
[[strategy]]
name = "fast"
path = "fast/strategy.py"
parameters = { window = 5 }

[[strategy]]
name = "slow"
path = "slow/strategy.py"
"#,
        );
        let jobs = build_catalog_jobs(
            "[backtest]\ndata = \"bars.csv\"\n[strategy_parameters]\nrisk = 0.1\n",
            Some(Path::new("config")),
            &catalog,
        )
        .unwrap();
        std::fs::remove_file(&catalog).unwrap();

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].name, "fast");
        assert_eq!(jobs[0].config.strategy_parameters["window"], 5);
        assert_eq!(jobs[0].config.strategy_parameters["risk"], 0.1);
        assert!(jobs[0]
            .config
            .strategy_path
            .as_ref()
            .unwrap()
            .ends_with("fast/strategy.py"));
    }
}
