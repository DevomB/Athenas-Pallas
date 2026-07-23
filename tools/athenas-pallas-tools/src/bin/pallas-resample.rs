//! Offline OHLCV bar aggregation (e.g. 1m CSV -> 30m).

use athenas_pallas::events::{Event, MarketEvent};
use athenas_pallas::OhlcvRow;
use clap::{Parser, ValueEnum};
use rust_decimal::Decimal;
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

#[derive(Parser, Debug)]
#[command(
    name = "pallas-resample",
    about = "Aggregate OHLCV CSV to a coarser interval"
)]
struct Args {
    /// Input CSV (`ts,open,high,low,close,volume`).
    #[arg(long)]
    input: PathBuf,
    /// Input representation.
    #[arg(long, value_enum, default_value_t = InputFormat::OhlcvCsv)]
    from: InputFormat,
    /// Target interval: `5m`, `15m`, `30m`, `1h`, `4h`, `1d`.
    #[arg(long)]
    to: String,
    /// Output CSV path.
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.input == args.output
        || (args.output.exists()
            && std::fs::canonicalize(&args.input)? == std::fs::canonicalize(&args.output)?)
    {
        return Err("output must not overwrite the source data".into());
    }
    let bucket_secs = interval_to_seconds(&args.to)
        .ok_or_else(|| format!("unsupported target interval: {}", args.to))?;
    let (rows, policy) = match args.from {
        InputFormat::OhlcvCsv => (load_rows(&args.input)?, None),
        InputFormat::TradesJsonl => {
            let events = athenas_pallas::backtest::read_events_jsonl(File::open(&args.input)?)?;
            let (rows, instrument) = trade_rows(events)?;
            (
                rows,
                Some(TradeBarPolicy {
                    version: 1,
                    source: args.input.display().to_string(),
                    instrument,
                    interval_seconds: bucket_secs,
                    timestamp: "UTC bucket start aligned to Unix epoch",
                    open: "first trade price",
                    high: "maximum trade price",
                    low: "minimum trade price",
                    close: "last trade price",
                    volume: "sum of trade quantity",
                    empty_buckets: "omitted",
                }),
            )
        }
    };
    if rows.is_empty() {
        return Err("empty input".into());
    }
    let aggregated = aggregate_rows(&rows, bucket_secs);
    write_ohlcv(&args.output, &aggregated)?;
    if let Some(policy) = policy {
        let policy_path = policy_path(&args.output);
        serde_json::to_writer_pretty(BufWriter::new(File::create(&policy_path)?), &policy)?;
        println!("wrote construction policy to {}", policy_path.display());
    }
    println!(
        "wrote {} bars ({} -> {}) to {}",
        aggregated.len(),
        rows.len(),
        args.to,
        args.output.display()
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum InputFormat {
    /// OHLCV CSV.
    OhlcvCsv,
    /// Engine JSONL containing one instrument's normalized trades.
    TradesJsonl,
}

#[derive(Serialize)]
struct TradeBarPolicy {
    version: u8,
    source: String,
    instrument: String,
    interval_seconds: i64,
    timestamp: &'static str,
    open: &'static str,
    high: &'static str,
    low: &'static str,
    close: &'static str,
    volume: &'static str,
    empty_buckets: &'static str,
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

fn load_rows(path: &Path) -> Result<Vec<ParsedRow>, Box<dyn std::error::Error>> {
    let mut rdr = csv::Reader::from_reader(BufReader::new(File::open(path)?));
    let mut out = Vec::new();
    for rec in rdr.deserialize::<OhlcvRow>() {
        let row = rec?;
        let ts = athenas_pallas::parse_timestamp(&row.ts)
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
    Ok(out)
}

fn trade_rows(events: Vec<Event>) -> Result<(Vec<ParsedRow>, String), Box<dyn std::error::Error>> {
    let mut rows = Vec::new();
    let mut instrument = None;
    let mut previous_ts = None;
    for event in events {
        let Event::Market(MarketEvent::Trade {
            instrument: current,
            ts,
            price,
            qty,
            ..
        }) = event
        else {
            return Err("trades-jsonl input must contain only trade events".into());
        };
        if price <= Decimal::ZERO || qty <= Decimal::ZERO {
            return Err("trade price and quantity must be positive".into());
        }
        if let Some(expected) = &instrument {
            if expected != &current {
                return Err(format!(
                    "trades-jsonl input mixes instruments {expected} and {current}"
                )
                .into());
            }
        } else {
            instrument = Some(current.clone());
        }
        if previous_ts.is_some_and(|previous| ts < previous) {
            return Err("trades-jsonl input must be ordered by event timestamp".into());
        }
        previous_ts = Some(ts);
        rows.push(ParsedRow {
            ts,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: qty,
        });
    }
    let instrument = instrument.ok_or("trades-jsonl input contains no trades")?;
    Ok((rows, instrument.to_string()))
}

fn bucket_start(ts: OffsetDateTime, bucket_secs: i64) -> i64 {
    ts.unix_timestamp().div_euclid(bucket_secs) * bucket_secs
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

fn write_ohlcv(path: &Path, rows: &[ParsedRow]) -> Result<(), Box<dyn std::error::Error>> {
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
        return rest.parse().ok().filter(|n: &i64| *n > 0).map(|n| n * 60);
    }
    if let Some(rest) = s.strip_suffix('h') {
        return rest
            .parse()
            .ok()
            .filter(|n: &i64| *n > 0)
            .map(|n| n * 3_600);
    }
    if let Some(rest) = s.strip_suffix('d') {
        return rest
            .parse()
            .ok()
            .filter(|n: &i64| *n > 0)
            .map(|n| n * 86_400);
    }
    None
}

fn policy_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_os_string();
    path.push(".policy.json");
    path.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use athenas_pallas::types::InstrumentId;
    use time::macros::datetime;

    fn trade(ts: OffsetDateTime, price: i64, qty: i64) -> Event {
        Event::Market(MarketEvent::Trade {
            instrument: InstrumentId::new("XNAS", "TEST"),
            ts,
            price: Decimal::new(price, 0),
            qty: Decimal::new(qty, 0),
            provenance: None,
        })
    }

    #[test]
    fn trades_build_deterministic_utc_bars() {
        let events = vec![
            trade(datetime!(2026-01-02 14:30:01 UTC), 100, 2),
            trade(datetime!(2026-01-02 14:30:59 UTC), 103, 3),
            trade(datetime!(2026-01-02 14:31:00 UTC), 101, 5),
        ];
        let (rows, instrument) = trade_rows(events).unwrap();
        let bars = aggregate_rows(&rows, 60);

        assert_eq!(instrument, "XNAS:TEST");
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].ts, datetime!(2026-01-02 14:30 UTC));
        assert_eq!(bars[0].open, Decimal::new(100, 0));
        assert_eq!(bars[0].high, Decimal::new(103, 0));
        assert_eq!(bars[0].low, Decimal::new(100, 0));
        assert_eq!(bars[0].close, Decimal::new(103, 0));
        assert_eq!(bars[0].volume, Decimal::new(5, 0));
        assert_eq!(bars[1].ts, datetime!(2026-01-02 14:31 UTC));
    }
}
