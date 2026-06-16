//! Merge multiple historical CSV sources into one time-ordered event stream CSV.

use athenas_pallas::backtest::sources::{FutureCsvSource, FxCsvSource, YahooCsvSource};
use athenas_pallas::backtest::{merge_sources, CsvBarSource, DataFormat, HistoricalSource};
use athenas_pallas::events::{Event, MarketEvent};
use athenas_pallas::types::{ExchangeId, Symbol};
use clap::Parser;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "pallas-merge",
    about = "Merge multiple OHLCV CSVs by timestamp"
)]
struct Args {
    /// `format:exchange:symbol:path` (repeatable). Format: ohlcv|yahoo|fx|future
    #[arg(long = "source", value_name = "SPEC")]
    sources: Vec<String>,
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.sources.is_empty() {
        return Err("at least one --source is required".into());
    }
    let mut boxes: Vec<Box<dyn HistoricalSource>> = Vec::with_capacity(args.sources.len());
    for spec in &args.sources {
        let (format, exchange, symbol, path) = parse_source_spec(spec)?;
        let src = open_source(format, &path, exchange, symbol)?;
        boxes.push(src);
    }
    let merged = merge_sources(&mut boxes);
    write_bar_csv(&args.output, &merged)?;
    println!(
        "wrote {} events from {} sources to {}",
        merged.len(),
        args.sources.len(),
        args.output.display()
    );
    Ok(())
}

fn parse_source_spec(
    spec: &str,
) -> Result<(DataFormat, ExchangeId, Symbol, PathBuf), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = spec.splitn(4, ':').collect();
    if parts.len() != 4 {
        return Err(
            "source spec must be format:exchange:symbol:path (e.g. yahoo:yahoo:AAPL:data/AAPL.csv)"
                .into(),
        );
    }
    let format = match parts[0].to_lowercase().as_str() {
        "ohlcv" | "binance" => DataFormat::Ohlcv,
        "yahoo" => DataFormat::Yahoo,
        "fx" => DataFormat::Fx,
        "future" | "futures" => DataFormat::Future,
        other => return Err(format!("unknown format in source spec: {other}").into()),
    };
    Ok((
        format,
        ExchangeId::new(parts[1]),
        Symbol::new(parts[2]),
        PathBuf::from(parts[3]),
    ))
}

fn open_source(
    format: DataFormat,
    path: &Path,
    exchange: ExchangeId,
    symbol: Symbol,
) -> Result<Box<dyn HistoricalSource>, Box<dyn std::error::Error>> {
    let src: Box<dyn HistoricalSource> = match format {
        DataFormat::Auto | DataFormat::Ohlcv => {
            Box::new(CsvBarSource::from_path(path, exchange, symbol)?)
        }
        DataFormat::Yahoo => Box::new(YahooCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Fx => Box::new(FxCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Future => Box::new(FutureCsvSource::from_path(path, exchange, symbol)?),
    };
    Ok(src)
}

fn write_bar_csv(path: &PathBuf, events: &[Event]) -> Result<(), Box<dyn std::error::Error>> {
    let mut w = BufWriter::new(File::create(path)?);
    writeln!(w, "ts,exchange,symbol,open,high,low,close,volume")?;
    for ev in events {
        if let Event::Market(MarketEvent::Bar {
            ts,
            instrument,
            open,
            high,
            low,
            close,
            volume,
        }) = ev
        {
            writeln!(
                w,
                "{},{},{},{},{},{},{},{}",
                ts.format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| ts.unix_timestamp().to_string()),
                instrument.exchange.as_str(),
                instrument.symbol.as_str(),
                open,
                high,
                low,
                close,
                volume
            )?;
        }
    }
    Ok(())
}
