//! Run a CSV backtest with optional external strategy script.

use athenas_pallas::backtest::{
    parse_asset_class, parse_data_format, parse_instrument, run_backtest, run_external_backtest,
    BacktestConfig, BacktestReport, DataFormat,
};
use clap::Parser;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "pallas-backtest")]
struct Args {
    #[arg(long)]
    data: Option<PathBuf>,
    #[arg(long)]
    provider: Option<String>,
    #[arg(long)]
    dataset: Option<String>,
    #[arg(long)]
    symbol: Option<String>,
    #[arg(long)]
    schema: Option<String>,
    #[arg(
        long,
        value_name = "MM-DD-YYYY",
        help = "Historical range start in American month-day-year format, e.g. 01-01-2025"
    )]
    start: Option<String>,
    #[arg(
        long,
        value_name = "MM-DD-YYYY",
        help = "Historical range end in American month-day-year format, e.g. 02-01-2025"
    )]
    end: Option<String>,
    #[arg(long, default_value = "raw_symbol")]
    stype_in: String,
    #[arg(long, default_value = "1.00")]
    cost_warning_usd: f64,
    #[arg(long)]
    estimate_only: bool,
    #[arg(long)]
    refresh_data: bool,
    #[arg(long, default_value = "data/databento")]
    cache_dir: PathBuf,
    #[arg(long)]
    yes: bool,
    #[arg(long, default_value = "test:EXAMPLE")]
    instrument: String,
    #[arg(long = "initial-balance", value_parser = parse_balance)]
    initial_balance: Vec<(String, String)>,
    #[arg(long)]
    strategy: Option<PathBuf>,
    #[arg(long, default_value = "python")]
    python: String,
    #[arg(long, default_value = "auto")]
    data_format: String,
    #[arg(long, default_value = "equity")]
    asset_class: String,
    #[arg(long, default_value = "10")]
    fee_bps: u64,
    #[arg(long, default_value = "5")]
    slippage_bps: u64,
    #[arg(long, default_value = "5")]
    half_spread_bps: u64,
    #[arg(long, value_parser = parse_positive_decimal)]
    buy_and_hold_qty: Option<Decimal>,
    #[arg(long = "param", value_name = "KEY=JSON", value_parser = parse_strategy_parameter)]
    strategy_parameter: Vec<(String, serde_json::Value)>,
    #[arg(long, default_value = "365")]
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

fn parse_strategy_parameter(s: &str) -> Result<(String, serde_json::Value), String> {
    let (key, raw) = s
        .split_once('=')
        .ok_or_else(|| "strategy parameter must be KEY=JSON".to_string())?;
    if key.trim().is_empty() {
        return Err("strategy parameter key must not be empty".into());
    }
    let value =
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
    Ok((key.to_string(), value))
}

fn parse_positive_decimal(s: &str) -> Result<Decimal, String> {
    let value = s
        .parse::<Decimal>()
        .map_err(|_| format!("invalid decimal `{s}`"))?;
    if value <= Decimal::ZERO {
        return Err("quantity must be positive".into());
    }
    Ok(value)
}

fn parse_balances(
    rows: &[(String, String)],
) -> Result<HashMap<athenas_pallas::types::Asset, Decimal>, Box<dyn std::error::Error>> {
    let mut balances = HashMap::new();
    for (a, v) in rows {
        balances.insert(athenas_pallas::types::Asset::new(a), v.parse()?);
    }
    Ok(balances)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let cli_balances = parse_balances(&args.initial_balance)?;
    let requested_data_format = parse_data_format(&args.data_format)?;
    let mut cfg = build_config(&args, &cli_balances, requested_data_format)?;
    apply_cli_overrides(&args, cli_balances, &mut cfg);
    if configure_provider(&args, requested_data_format, &mut cfg)? {
        return Ok(());
    }
    if cfg.data_path.as_os_str().is_empty() {
        return Err("missing --data or [backtest].data in config".into());
    }
    let report = run(&cfg)?;
    print_report(&report);
    write_report(&cfg, &report)?;
    Ok(())
}

fn build_config(
    args: &Args,
    balances: &HashMap<athenas_pallas::types::Asset, Decimal>,
    data_format: DataFormat,
) -> Result<BacktestConfig, Box<dyn std::error::Error>> {
    if let Some(path) = &args.config {
        return Ok(BacktestConfig::load_toml(path)?);
    }
    let instrument = parse_instrument(&args.instrument)?;
    Ok(BacktestConfig {
        data_path: args.data.clone().unwrap_or_default(),
        data_format,
        instrument,
        asset_class: parse_asset_class(&args.asset_class)?,
        balances: if balances.is_empty() {
            HashMap::from([(
                athenas_pallas::types::Asset::new("USD"),
                Decimal::new(10_000, 0),
            )])
        } else {
            balances.clone()
        },
        fee_bps: Decimal::from(args.fee_bps),
        slippage_bps: Decimal::from(args.slippage_bps),
        half_spread_bps: Decimal::from(args.half_spread_bps),
        buy_and_hold_qty: args.buy_and_hold_qty,
        periods_per_year: args.periods_per_year,
        strategy_path: args.strategy.clone(),
        strategy_parameters: args.strategy_parameter.iter().cloned().collect(),
        python_exe: args.python.clone(),
        output_path: args.output.clone(),
        verbose: args.verbose,
        ..BacktestConfig::default()
    })
}

fn apply_cli_overrides(
    args: &Args,
    balances: HashMap<athenas_pallas::types::Asset, Decimal>,
    cfg: &mut BacktestConfig,
) {
    if let Some(data) = &args.data {
        cfg.data_path = data.clone();
    }
    if !balances.is_empty() {
        cfg.balances = balances;
    }
    if let Some(strategy) = &args.strategy {
        cfg.strategy_path = Some(strategy.clone());
    }
    if let Some(qty) = args.buy_and_hold_qty {
        cfg.buy_and_hold_qty = Some(qty);
    }
    cfg.strategy_parameters
        .extend(args.strategy_parameter.iter().cloned());
    if args.verbose {
        cfg.verbose = true;
    }
    if let Some(output) = &args.output {
        cfg.output_path = Some(output.clone());
    }
}

fn configure_provider(
    args: &Args,
    data_format: DataFormat,
    cfg: &mut BacktestConfig,
) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(provider) = args.provider.as_deref() else {
        return Ok(false);
    };
    if !provider.trim().eq_ignore_ascii_case("databento") {
        return Err(format!("unsupported --provider '{provider}'").into());
    }
    configure_databento(args, data_format, cfg)
}

#[cfg(not(feature = "databento"))]
fn configure_databento(
    _: &Args,
    _: DataFormat,
    _: &mut BacktestConfig,
) -> Result<bool, Box<dyn std::error::Error>> {
    Err("databento provider requested, but this binary was built without the 'databento' feature; rerun with --features databento".into())
}

#[cfg(feature = "databento")]
fn configure_databento(
    args: &Args,
    data_format: DataFormat,
    cfg: &mut BacktestConfig,
) -> Result<bool, Box<dyn std::error::Error>> {
    use athenas_pallas::data::databento::{cache_path, ensure_cached_csv};

    if args.data.is_some() {
        return Err("databento manages its cache path; omit --data or --provider".into());
    }
    if !matches!(data_format, DataFormat::Auto | DataFormat::Ohlcv) {
        return Err("databento writes OHLCV; use --data-format auto or ohlcv".into());
    }
    let fetch = databento_config(args)?;
    let planned_path = cache_path(&fetch);
    let result = ensure_cached_csv(&fetch)?;
    if args.estimate_only {
        return Ok(true);
    }
    cfg.data_path = result.cache_path;
    cfg.data_format = DataFormat::Ohlcv;
    if args.instrument == "test:EXAMPLE" {
        cfg.instrument = parse_instrument(&format!("databento:{}", fetch.symbol))?;
    }
    if cfg.verbose {
        let action = if result.fetched {
            "cached"
        } else {
            "using cached"
        };
        eprintln!("{action} Databento data at {}", cfg.data_path.display());
    }
    if planned_path != cfg.data_path {
        return Err("internal databento cache path mismatch".into());
    }
    Ok(false)
}

#[cfg(feature = "databento")]
fn databento_config(
    args: &Args,
) -> Result<athenas_pallas::data::databento::DatabentoFetchConfig, Box<dyn std::error::Error>> {
    use athenas_pallas::data::databento::{
        parse_datetime, DatabentoFetchConfig, DatabentoOhlcvSchema, DatabentoSType,
    };

    Ok(DatabentoFetchConfig {
        dataset: args.dataset.clone().ok_or("missing --dataset")?,
        symbol: args.symbol.clone().ok_or("missing --symbol")?,
        schema: DatabentoOhlcvSchema::parse(args.schema.as_deref().ok_or("missing --schema")?)?,
        start: parse_datetime(args.start.as_deref().ok_or("missing --start")?)?,
        end: parse_datetime(args.end.as_deref().ok_or("missing --end")?)?,
        stype_in: DatabentoSType::parse(&args.stype_in)?,
        cache_dir: args.cache_dir.clone(),
        refresh_data: args.refresh_data,
        cost_warning_usd: args.cost_warning_usd,
        yes: args.yes,
        estimate_only: args.estimate_only,
    })
}

fn run(cfg: &BacktestConfig) -> Result<BacktestReport, Box<dyn std::error::Error>> {
    let report = if let Some(strategy_path) = &cfg.strategy_path {
        run_external_backtest(cfg, strategy_path)?
    } else {
        run_backtest(cfg)?
    };
    Ok(report)
}

fn print_report(report: &BacktestReport) {
    println!("PnL: {}", report.pnl);
    println!("PnL %: {}", report.pnl_pct);
    println!("Sharpe: {}", report.sharpe);
    println!("Sortino: {}", report.sortino);
    println!("Max drawdown: {}", report.max_drawdown);
    println!("fills: {}", report.fill_count);
    println!("fees: {}", report.total_fees);
    println!("turnover: {}", report.turnover);
    println!(
        "rejections: {} risk, {} execution",
        report.risk_rejection_count, report.execution_rejection_count
    );
    println!("pending orders: {}", report.pending_orders.len());
}

fn write_report(
    cfg: &BacktestConfig,
    report: &BacktestReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(out) = &cfg.output_path {
        report.write_json(out)?;
        if cfg.verbose {
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}
