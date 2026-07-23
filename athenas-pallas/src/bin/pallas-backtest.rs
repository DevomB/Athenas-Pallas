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
    #[arg(
        long,
        default_value = "raw",
        help = "OHLCV adjustment policy: raw, split-adjusted, or total-return-adjusted"
    )]
    adjustment: String,
    #[arg(long, help = "Fetch and apply point-in-time instrument definitions")]
    import_definitions: bool,
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
    #[arg(
        long,
        default_value = "auto",
        help = "Input layout: auto, ohlcv, fx, or jsonl"
    )]
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
    #[arg(long, value_parser = parse_positive_f64)]
    periods_per_year: Option<f64>,
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

fn parse_positive_f64(s: &str) -> Result<f64, String> {
    let value = s
        .parse::<f64>()
        .map_err(|_| format!("invalid number `{s}`"))?;
    if value <= 0.0 || !value.is_finite() {
        return Err("periods per year must be positive and finite".into());
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
    if let Some(periods_per_year) = args.periods_per_year {
        cfg.periods_per_year = periods_per_year;
        cfg.auto_periods_per_year = false;
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
    let fetch = databento_config(args)?;
    let provider_format = if fetch.schema.is_ohlcv() {
        DataFormat::Ohlcv
    } else {
        DataFormat::Jsonl
    };
    if !matches!(data_format, DataFormat::Auto) && data_format != provider_format {
        return Err(format!(
            "Databento schema '{}' requires --data-format {} or auto",
            fetch.schema.as_str(),
            match provider_format {
                DataFormat::Ohlcv => "ohlcv",
                DataFormat::Jsonl => "jsonl",
                _ => unreachable!(),
            }
        )
        .into());
    }
    let planned_path = cache_path(&fetch);
    let result = ensure_cached_csv(&fetch)?;
    if args.estimate_only {
        return Ok(true);
    }
    cfg.data_path = result.cache_path;
    cfg.data_format = provider_format;
    if let Some(path) = result.definitions_path {
        apply_databento_definition(
            cfg,
            athenas_pallas::data::databento::load_definition_for_symbol(&path, &fetch.symbol)?,
        )?;
    }
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
        parse_datetime, AdjustmentMode, DatabentoFetchConfig, DatabentoOhlcvSchema, DatabentoSType,
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
        adjustment_mode: AdjustmentMode::parse(&args.adjustment)?,
        import_definitions: args.import_definitions,
    })
}

#[cfg(feature = "databento")]
fn apply_databento_definition(
    cfg: &mut BacktestConfig,
    definition: athenas_pallas::data::databento::DatabentoInstrumentDefinition,
) -> Result<(), Box<dyn std::error::Error>> {
    use athenas_pallas::backtest::config::parse_option_kind;

    if definition.option_kind.is_some() {
        return Err(athenas_pallas::Error::Invalid(
            "Databento option definitions do not identify exercise style; replay requires explicit verified European-option metadata".into(),
        )
        .into());
    }
    cfg.asset_class = parse_asset_class(&definition.asset_class)?;
    cfg.instrument =
        athenas_pallas::types::InstrumentId::new("databento", definition.raw_symbol.clone());
    cfg.base_asset = Some(definition.raw_symbol);
    cfg.quote_asset = Some(definition.currency);
    cfg.tick_size = Some(definition.tick_size.parse()?);
    cfg.lot_size = Some(definition.lot_size.parse()?);
    cfg.contract_multiplier = definition
        .contract_multiplier
        .map(|value| value.parse())
        .transpose()?;
    cfg.expiry = definition.expiration;
    cfg.option_kind = definition
        .option_kind
        .map(|value| parse_option_kind(&value).map_err(athenas_pallas::Error::Invalid))
        .transpose()?;
    cfg.option_strike = definition
        .option_strike
        .map(|value| value.parse())
        .transpose()?;
    cfg.option_underlying = definition
        .option_underlying
        .map(|symbol| athenas_pallas::types::InstrumentId::new("databento", symbol));
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from_args(args: &[&str]) -> BacktestConfig {
        let args = Args::try_parse_from(args).unwrap();
        let balances = HashMap::new();
        let format = parse_data_format(&args.data_format).unwrap();
        let mut config = build_config(&args, &balances, format).unwrap();
        apply_cli_overrides(&args, balances, &mut config);
        config
    }

    #[test]
    fn omitted_periods_per_year_preserves_auto_inference() {
        let config = config_from_args(&["pallas-backtest"]);
        assert!(config.auto_periods_per_year);
        assert_eq!(config.periods_per_year, 365.0);
    }

    #[test]
    fn explicit_periods_per_year_disables_auto_inference() {
        let config = config_from_args(&["pallas-backtest", "--periods-per-year", "252"]);
        assert!(!config.auto_periods_per_year);
        assert_eq!(config.periods_per_year, 252.0);
    }

    #[test]
    fn periods_per_year_must_be_positive_and_finite() {
        assert!(Args::try_parse_from(["pallas-backtest", "--periods-per-year", "0"]).is_err());
        assert!(Args::try_parse_from(["pallas-backtest", "--periods-per-year", "NaN"]).is_err());
    }

    #[cfg(feature = "databento")]
    fn definition(
        asset_class: &str,
        option_kind: Option<&str>,
    ) -> athenas_pallas::data::databento::DatabentoInstrumentDefinition {
        athenas_pallas::data::databento::DatabentoInstrumentDefinition {
            ts_recv: "2026-01-01T00:00:00Z".into(),
            publisher_id: 1,
            instrument_id: 2,
            raw_symbol: "ESM6".into(),
            asset_class: asset_class.into(),
            currency: "USD".into(),
            tick_size: "0.25".into(),
            lot_size: "1".into(),
            contract_multiplier: Some("50".into()),
            expiration: Some("2026-06-19T00:00:00Z".into()),
            option_kind: option_kind.map(str::to_owned),
            option_strike: option_kind.map(|_| "6000".into()),
            option_underlying: option_kind.map(|_| "ESM6".into()),
            update_action: "add".into(),
        }
    }

    #[cfg(feature = "databento")]
    #[test]
    fn databento_definition_application_fails_closed_for_options() {
        let mut config = BacktestConfig::default();
        apply_databento_definition(&mut config, definition("future", None)).unwrap();
        assert_eq!(
            config.asset_class,
            athenas_pallas::instrument::AssetClass::Future
        );
        assert_eq!(config.contract_multiplier, Some(Decimal::from(50u64)));

        let error = apply_databento_definition(&mut config, definition("option", Some("call")))
            .unwrap_err();
        assert!(error.to_string().contains("do not identify exercise style"));
    }
}
