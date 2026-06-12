//! Run a CSV backtest with optional external strategy script.

use athenas_pallas::backtest::{
    parse_base_quote, parse_instrument, BacktestConfig, BacktestRunner, DataFormat,
};
use athenas_pallas::instrument::{AssetClass, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::strategy::ExternalStrategy;
use clap::Parser;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "pallas-backtest")]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long, default_value = "binance:BTCUSDT")]
    instrument: String,
    #[arg(long = "initial-balance", value_parser = parse_balance)]
    initial_balance: Vec<(String, String)>,
    #[arg(long)]
    strategy: Option<PathBuf>,
    #[arg(long, default_value = "python")]
    python: String,
    #[arg(long, default_value = "auto")]
    data_format: String,
    #[arg(long, default_value = "crypto")]
    asset_class: String,
    #[arg(long, default_value = "10")]
    fee_bps: u64,
    #[arg(long, default_value = "5")]
    slippage_bps: u64,
    #[arg(long, default_value = "252")]
    periods_per_year: f64,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(short, long)]
    verbose: bool,
    #[arg(long)]
    config: Option<PathBuf>,
}

fn parse_balance(s: &str) -> Result<(String, String), String> {
    let (a, v) = s
        .split_once(':')
        .ok_or_else(|| "balance must be ASSET:AMOUNT".to_string())?;
    Ok((a.to_string(), v.to_string()))
}

fn parse_asset_class(s: &str) -> AssetClass {
    match s.to_lowercase().as_str() {
        "equity" => AssetClass::Equity,
        "forex" | "fx" => AssetClass::Forex,
        "future" | "futures" => AssetClass::Future,
        _ => AssetClass::Crypto,
    }
}

fn apply_toml_config(cfg: &mut BacktestConfig, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let table: toml::Table = toml::from_str(&text)?;
    if let Some(fee) = table.get("fee_bps").and_then(|v| v.as_integer()) {
        cfg.fee_bps = Decimal::from(fee as u64);
    }
    if let Some(slip) = table.get("slippage_bps").and_then(|v| v.as_integer()) {
        cfg.slippage_bps = Decimal::from(slip as u64);
    }
    if let Some(inst) = table.get("instrument").and_then(|v| v.as_table()) {
        if let Some(ac) = inst.get("asset_class").and_then(|v| v.as_str()) {
            cfg.asset_class = parse_asset_class(ac);
        }
    }
    Ok(())
}

fn parse_data_format(s: &str) -> DataFormat {
    match s.to_lowercase().as_str() {
        "ohlcv" => DataFormat::Ohlcv,
        "yahoo" => DataFormat::Yahoo,
        "fx" => DataFormat::Fx,
        _ => DataFormat::Auto,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let instrument = parse_instrument(&args.instrument)?;
    let asset_class = parse_asset_class(&args.asset_class);
    let mut balances = HashMap::new();
    if args.initial_balance.is_empty() {
        balances.insert(
            athenas_pallas::types::Asset::new("USDT"),
            Decimal::new(10_000, 0),
        );
    } else {
        for (a, v) in args.initial_balance {
            let d: Decimal = v.parse()?;
            balances.insert(athenas_pallas::types::Asset::new(a), d);
        }
    }

    let mut cfg = BacktestConfig {
        data_path: args.data,
        data_format: parse_data_format(&args.data_format),
        instrument: instrument.clone(),
        asset_class,
        balances,
        fee_bps: Decimal::from(args.fee_bps),
        slippage_bps: Decimal::from(args.slippage_bps),
        periods_per_year: args.periods_per_year,
        strategy_path: args.strategy.clone(),
        python_exe: args.python,
        output_path: args.output.clone(),
        verbose: args.verbose,
        ..BacktestConfig::default()
    };
    if let Some(path) = args.config {
        apply_toml_config(&mut cfg, &path)?;
    }

    let report = if let Some(ref strategy_path) = args.strategy {
        run_external(&cfg, strategy_path)?
    } else {
        BacktestRunner::run_buy_and_hold(&cfg)?
    };

    println!("PnL: {}", report.pnl);
    println!("fills: {}", report.fill_count);
    if let Some(out) = args.output {
        report.write_json(&out)?;
        if args.verbose {
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}

fn run_external(
    cfg: &BacktestConfig,
    strategy_path: &std::path::Path,
) -> athenas_pallas::Result<athenas_pallas::backtest::BacktestReport> {
    let (base, quote) = parse_base_quote(&cfg.instrument.symbol, cfg.asset_class);
    let mut instruments = HashMap::new();
    instruments.insert(
        cfg.instrument.clone(),
        InstrumentMeta {
            base: athenas_pallas::types::Asset::new(base),
            quote: athenas_pallas::types::Asset::new(quote),
            asset_class: cfg.asset_class,
            lot_size: None,
        },
    );
    let registry = InstrumentRegistry::from_instruments(instruments);
    let meta = registry
        .meta_by_id(&cfg.instrument)
        .expect("meta")
        .clone();
    let balances = if cfg.balances.is_empty() {
        let mut b = HashMap::new();
        b.insert(
            athenas_pallas::types::Asset::new("USDT"),
            Decimal::new(10_000, 0),
        );
        b
    } else {
        cfg.balances.clone()
    };

    let script = resolve_strategy_path(strategy_path)?;
    let mut ext = if script.extension().and_then(|e| e.to_str()) == Some("py") {
        ExternalStrategy::spawn_python(&script, &cfg.python_exe)?
    } else if strategy_path.join("CMakeLists.txt").is_file() {
        let binary = build_cpp_strategy(strategy_path)?;
        ExternalStrategy::spawn_binary(&binary)?
    } else {
        ExternalStrategy::spawn_binary(&script)?
    };
    ext.handshake(
        cfg.instrument.clone(),
        &meta,
        &balances,
        cfg.fee_bps,
    )?;
    let report = BacktestRunner::run_with_strategy(cfg, &mut ext)?;
    ext.take_error()?;
    Ok(report)
}

fn build_cpp_strategy(dir: &std::path::Path) -> athenas_pallas::Result<PathBuf> {
    let build_dir = dir.join("build");
    std::fs::create_dir_all(&build_dir).map_err(athenas_pallas::Error::Io)?;
    let status = std::process::Command::new("cmake")
        .arg("-S")
        .arg(dir)
        .arg("-B")
        .arg(&build_dir)
        .status()
        .map_err(athenas_pallas::Error::Io)?;
    if !status.success() {
        return Err(athenas_pallas::Error::Invalid("cmake configure failed".into()));
    }
    let status = std::process::Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .status()
        .map_err(athenas_pallas::Error::Io)?;
    if !status.success() {
        return Err(athenas_pallas::Error::Invalid("cmake build failed".into()));
    }
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("strategy");
    let bin = if cfg!(windows) {
        build_dir.join("Release").join(format!("{name}.exe"))
    } else {
        build_dir.join(name)
    };
    if !bin.is_file() {
        let alt = build_dir.join(name);
        if alt.is_file() {
            return Ok(alt);
        }
        return Err(athenas_pallas::Error::Invalid(format!(
            "built binary not found at {}",
            bin.display()
        )));
    }
    Ok(bin)
}

fn resolve_strategy_path(path: &std::path::Path) -> athenas_pallas::Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if path.is_dir() {
        let py = path.join("strategy.py");
        if py.is_file() {
            return Ok(py);
        }
        let main_py = path.join("main.py");
        if main_py.is_file() {
            return Ok(main_py);
        }
    }
    Err(athenas_pallas::Error::Invalid(format!(
        "no strategy script at {}",
        path.display()
    )))
}
