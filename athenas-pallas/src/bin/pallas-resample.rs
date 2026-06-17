//! Offline OHLCV bar aggregation (e.g. 1m CSV -> 30m).

use athenas_pallas::backtest::OhlcvRow;
use clap::Parser;
use rust_decimal::Decimal;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use time::OffsetDateTime;

#[derive(Parser, Debug)]
#[command(
    name = "pallas-resample",
    about = "Aggregate OHLCV CSV to a coarser interval"
)]
struct Args {
    /// Input CSV (`ts,open,high,low,close,volume` or Yahoo `Date,...`).
    #[arg(long)]
    input: PathBuf,
    /// Target interval: `5m`, `15m`, `30m`, `1h`, `4h`, `1d`.
    #[arg(long)]
    to: String,
    /// Output CSV path.
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let bucket_secs = interval_to_seconds(&args.to)
        .ok_or_else(|| format!("unsupported target interval: {}", args.to))?;
    let rows = load_rows(&args.input)?;
    if rows.is_empty() {
        return Err("empty input".into());
    }
    let aggregated = aggregate_rows(&rows, bucket_secs);
    write_ohlcv(&args.output, &aggregated)?;
    println!(
        "wrote {} bars ({} -> {}) to {}",
        aggregated.len(),
        rows.len(),
        args.to,
        args.output.display()
    );
    Ok(())
}

#[derive(Clone)]
struct ParsedRow {
    ts: OffsetDateTime,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

fn load_rows(path: &PathBuf) -> Result<Vec<ParsedRow>, Box<dyn std::error::Error>> {
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut File::open(path)?, &mut buf)?;
    let mut rdr = csv::Reader::from_reader(buf.as_bytes());
    let headers = rdr.headers()?.clone();
    let yahoo = headers.iter().any(|h| h == "Date");
    let mut out = Vec::new();
    if yahoo {
        #[derive(serde::Deserialize)]
        struct YahooRow {
            #[serde(rename = "Date")]
            date: String,
            #[serde(rename = "Open")]
            open: Decimal,
            #[serde(rename = "High")]
            high: Decimal,
            #[serde(rename = "Low")]
            low: Decimal,
            #[serde(rename = "Close")]
            close: Decimal,
            #[serde(rename = "Volume")]
            volume: Decimal,
        }
        for rec in rdr.deserialize::<YahooRow>() {
            let row = rec?;
            let ts = athenas_pallas::backtest::parse_timestamp(&row.date)
                .ok_or_else(|| format!("bad timestamp: {}", row.date))?;
            out.push(ParsedRow {
                ts,
                open: row.open,
                high: row.high,
                low: row.low,
                close: row.close,
                volume: row.volume,
            });
        }
    } else {
        for rec in rdr.deserialize::<OhlcvRow>() {
            let row: OhlcvRow = rec?;
            let ts = athenas_pallas::backtest::parse_timestamp(&row.ts)
                .ok_or_else(|| format!("bad timestamp: {}", row.ts))?;
            out.push(ParsedRow {
                ts,
                open: row.open,
                high: row.high,
                low: row.low,
                close: row.close,
                volume: row.volume,
            });
        }
    }
    Ok(out)
}

fn bucket_start(ts: OffsetDateTime, bucket_secs: i64) -> i64 {
    ts.unix_timestamp() / bucket_secs * bucket_secs
}

fn aggregate_rows(rows: &[ParsedRow], bucket_secs: i64) -> Vec<ParsedRow> {
    let mut out: Vec<ParsedRow> = Vec::new();
    let mut cur_bucket: Option<i64> = None;
    for row in rows {
        let b = bucket_start(row.ts, bucket_secs);
        if cur_bucket != Some(b) {
            let ts = OffsetDateTime::from_unix_timestamp(b).unwrap_or(row.ts);
            out.push(ParsedRow {
                ts,
                open: row.open,
                high: row.high,
                low: row.low,
                close: row.close,
                volume: row.volume,
            });
            cur_bucket = Some(b);
        } else if let Some(last) = out.last_mut() {
            last.high = last.high.max(row.high);
            last.low = last.low.min(row.low);
            last.close = row.close;
            last.volume += row.volume;
        }
    }
    out
}

fn write_ohlcv(path: &PathBuf, rows: &[ParsedRow]) -> Result<(), Box<dyn std::error::Error>> {
    let mut w = BufWriter::new(File::create(path)?);
    writeln!(w, "ts,open,high,low,close,volume")?;
    for row in rows {
        writeln!(
            w,
            "{},{},{},{},{},{}",
            row.ts
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| row.ts.unix_timestamp().to_string()),
            row.open,
            row.high,
            row.low,
            row.close,
            row.volume
        )?;
    }
    Ok(())
}

fn interval_to_seconds(interval: &str) -> Option<i64> {
    let s = interval.trim().to_lowercase();
    if let Some(rest) = s.strip_suffix('m') {
        return rest.parse().ok().map(|n: i64| n * 60);
    }
    if let Some(rest) = s.strip_suffix('h') {
        return rest.parse().ok().map(|n: i64| n * 3_600);
    }
    if let Some(rest) = s.strip_suffix('d') {
        return rest.parse().ok().map(|n: i64| n * 86_400);
    }
    match s.as_str() {
        "1h" | "60m" => Some(3_600),
        "1d" => Some(86_400),
        _ => None,
    }
}
