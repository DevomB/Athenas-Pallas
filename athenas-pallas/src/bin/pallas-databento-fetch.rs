//! Fetch Databento OHLCV history into the engine CSV format.

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::str::FromStr;

use athenas_pallas::backtest::parse_timestamp;
use athenas_pallas::data::databento::{fetch_ohlcv_csv, DatabentoOhlcvRequest};
use clap::Parser;
use databento::dbn::{SType, Schema};
use time::OffsetDateTime;

#[derive(Parser, Debug)]
#[command(
    name = "pallas-databento-fetch",
    about = "Fetch Databento historical OHLCV bars as ts,open,high,low,close,volume CSV"
)]
struct Args {
    /// Databento dataset code, e.g. GLBX.MDP3, XNAS.ITCH, EQUS.MINI.
    #[arg(long)]
    dataset: String,
    /// Databento symbol or symbol expression.
    #[arg(long)]
    symbol: String,
    /// Input symbology: raw_symbol, parent, continuous, nasdaq_symbol, instrument_id, etc.
    #[arg(long, default_value = "raw_symbol", value_parser = parse_stype)]
    stype_in: SType,
    /// OHLCV schema: ohlcv-1s, ohlcv-1m, ohlcv-1h, or ohlcv-1d.
    #[arg(long, default_value = "ohlcv-1d", value_parser = parse_schema)]
    schema: Schema,
    /// Inclusive UTC start timestamp. RFC3339 is recommended.
    #[arg(long, value_parser = parse_time)]
    start: OffsetDateTime,
    /// Exclusive UTC end timestamp. RFC3339 is recommended.
    #[arg(long, value_parser = parse_time)]
    end: OffsetDateTime,
    /// Output CSV path for pallas-backtest.
    #[arg(short, long)]
    output: PathBuf,
    /// Optional sanity guard for expected maximum rows. The official client still controls request size.
    #[arg(long)]
    expect_at_most: Option<NonZeroU64>,
}

fn parse_schema(s: &str) -> Result<Schema, String> {
    let schema = Schema::from_str(&s.to_lowercase()).map_err(|e| e.to_string())?;
    if matches!(
        schema,
        Schema::Ohlcv1S | Schema::Ohlcv1M | Schema::Ohlcv1H | Schema::Ohlcv1D
    ) {
        Ok(schema)
    } else {
        Err("schema must be one of ohlcv-1s, ohlcv-1m, ohlcv-1h, ohlcv-1d".to_string())
    }
}

fn parse_stype(s: &str) -> Result<SType, String> {
    SType::from_str(&s.to_lowercase()).map_err(|e| e.to_string())
}

fn parse_time(s: &str) -> Result<OffsetDateTime, String> {
    parse_timestamp(s)
        .ok_or_else(|| "timestamp must be RFC3339, YYYY-MM-DD HH:MM:SS, or YYYY-MM-DD".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let req = DatabentoOhlcvRequest {
        dataset: args.dataset,
        symbol: args.symbol,
        stype_in: args.stype_in,
        schema: args.schema,
        start: args.start,
        end: args.end,
    };
    let rows = fetch_ohlcv_csv(&req, &args.output).await?;
    if let Some(limit) = args.expect_at_most {
        if rows > limit.get() {
            return Err(format!("wrote {rows} rows, exceeding --expect-at-most {limit}").into());
        }
    }
    println!("wrote {rows} rows to {}", args.output.display());
    Ok(())
}
