//! Run a CSV backtest with optional external strategy script.

use athenas_pallas::backtest::{
    parse_instrument, run_backtest, run_external_backtest, BacktestConfig, DataFormat,
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
    #[arg(long)]
    start: Option<String>,
    #[arg(long)]
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

fn parse_asset_class(s: &str) -> athenas_pallas::instrument::AssetClass {
    match s.to_lowercase().as_str() {
        "equity" => athenas_pallas::instrument::AssetClass::Equity,
        "forex" | "fx" => athenas_pallas::instrument::AssetClass::Forex,
        "future" | "futures" => athenas_pallas::instrument::AssetClass::Future,
        _ => athenas_pallas::instrument::AssetClass::Crypto,
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
    let has_cli_balances = !args.initial_balance.is_empty();
    let requested_data_format = parse_data_format(&args.data_format);

    let mut cfg = if let Some(path) = &args.config {
        BacktestConfig::load_toml(path)?
    } else {
        let instrument = parse_instrument(&args.instrument)?;
        BacktestConfig {
            data_path: args.data.clone().unwrap_or_default(),
            data_format: requested_data_format,
            instrument,
            asset_class: parse_asset_class(&args.asset_class),
            balances: if has_cli_balances {
                cli_balances.clone()
            } else {
                let mut b = HashMap::new();
                b.insert(
                    athenas_pallas::types::Asset::new("USD"),
                    Decimal::new(10_000, 0),
                );
                b
            },
            fee_bps: Decimal::from(args.fee_bps),
            slippage_bps: Decimal::from(args.slippage_bps),
            half_spread_bps: Decimal::from(args.half_spread_bps),
            periods_per_year: args.periods_per_year,
            strategy_path: args.strategy.clone(),
            python_exe: args.python.clone(),
            output_path: args.output.clone(),
            verbose: args.verbose,
            ..BacktestConfig::default()
        }
    };

    if let Some(data) = &args.data {
        cfg.data_path = data.clone();
    }
    if has_cli_balances {
        cfg.balances = cli_balances;
    }
    if let Some(s) = args.strategy {
        cfg.strategy_path = Some(s);
    }
    if args.verbose {
        cfg.verbose = true;
    }
    if let Some(out) = args.output {
        cfg.output_path = Some(out);
    }

    if let Some(provider) = &args.provider {
        match provider.trim().to_ascii_lowercase().as_str() {
            "databento" => {
                #[cfg(not(feature = "databento"))]
                {
                    return Err(
                        "databento provider requested, but this binary was built without the 'databento' feature; rerun with --features databento"
                            .into(),
                    );
                }
                #[cfg(feature = "databento")]
                {
                    use athenas_pallas::data::databento::{
                        cache_path, ensure_cached_csv, parse_datetime, DatabentoFetchConfig,
                        DatabentoOhlcvSchema, DatabentoSType,
                    };

                    if args.data.is_some() {
                        return Err(
                            "databento provider writes and reuses its own cache path; omit --data or run without --provider"
                                .into(),
                        );
                    }
                    if !matches!(requested_data_format, DataFormat::Auto | DataFormat::Ohlcv) {
                        return Err(
                            "databento provider writes OHLCV CSV; use --data-format auto or --data-format ohlcv"
                                .into(),
                        );
                    }
                    let dataset = args
                        .dataset
                        .clone()
                        .ok_or("missing --dataset for --provider databento")?;
                    let symbol = args
                        .symbol
                        .clone()
                        .ok_or("missing --symbol for --provider databento")?;
                    let schema = DatabentoOhlcvSchema::parse(
                        args.schema
                            .as_deref()
                            .ok_or("missing --schema for --provider databento")?,
                    )?;
                    let start = parse_datetime(
                        args.start
                            .as_deref()
                            .ok_or("missing --start for --provider databento")?,
                    )?;
                    let end = parse_datetime(
                        args.end
                            .as_deref()
                            .ok_or("missing --end for --provider databento")?,
                    )?;
                    let fetch_cfg = DatabentoFetchConfig {
                        dataset,
                        symbol,
                        schema,
                        start,
                        end,
                        stype_in: DatabentoSType::parse(&args.stype_in)?,
                        cache_dir: args.cache_dir.clone(),
                        refresh_data: args.refresh_data,
                        cost_warning_usd: args.cost_warning_usd,
                        yes: args.yes,
                        estimate_only: args.estimate_only,
                    };
                    let planned_cache_path = cache_path(&fetch_cfg);
                    let result = ensure_cached_csv(&fetch_cfg)?;
                    if args.estimate_only {
                        return Ok(());
                    }
                    cfg.data_path = result.cache_path;
                    cfg.data_format = DataFormat::Ohlcv;
                    if args.instrument == "test:EXAMPLE" {
                        cfg.instrument =
                            parse_instrument(&format!("databento:{}", fetch_cfg.symbol))?;
                    }
                    if cfg.verbose {
                        if result.fetched {
                            eprintln!("cached Databento data at {}", cfg.data_path.display());
                        } else {
                            eprintln!("using cached Databento data at {}", cfg.data_path.display());
                        }
                    }
                    if planned_cache_path != cfg.data_path {
                        return Err("internal databento cache path mismatch".into());
                    }
                }
            }
            other => return Err(format!("unsupported --provider '{other}'").into()),
        }
    }

    if cfg.data_path.as_os_str().is_empty() {
        return Err("missing --data or [backtest].data in config".into());
    }

    let strategy_path = cfg.strategy_path.clone();
    let report = if let Some(ref strategy_path) = strategy_path {
        run_external_backtest(&cfg, strategy_path)?
    } else {
        run_backtest(&cfg)?
    };

    println!("PnL: {}", report.pnl);
    println!("PnL %: {}", report.pnl_pct);
    println!("Sharpe: {}", report.sharpe);
    println!("Sortino: {}", report.sortino);
    println!("Max drawdown: {}", report.max_drawdown);
    println!("fills: {}", report.fill_count);
    if let Some(out) = cfg.output_path {
        report.write_json(&out)?;
        if cfg.verbose {
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}
