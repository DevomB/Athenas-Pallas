use rust_decimal::Decimal;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

use athenas_pallas::backtest::sources::{FutureCsvSource, FxCsvSource, YahooCsvSource};
use athenas_pallas::backtest::{
    merge_sources_iter, CsvBarSource, DataFormat, HistoricalSource, OhlcvRow,
};
use athenas_pallas::events::{Event, MarketEvent};
use athenas_pallas::types::{ExchangeId, Symbol};

use crate::dto::{CsvPreviewDto, MergeRequest, ResampleRequest};

pub fn resample_bars(req: &ResampleRequest) -> Result<String, String> {
    let bucket_secs = interval_to_seconds(&req.target_interval)
        .ok_or_else(|| format!("unsupported interval: {}", req.target_interval))?;
    let input = PathBuf::from(&req.input_path);
    let output = PathBuf::from(&req.output_path);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let rows = load_rows(&input)?;
    if rows.is_empty() {
        return Err("empty input".into());
    }
    let aggregated = aggregate_rows(&rows, bucket_secs);
    write_ohlcv(&output, &aggregated)?;
    Ok(output.display().to_string())
}

pub fn merge_bars(req: &MergeRequest) -> Result<String, String> {
    let output = PathBuf::from(&req.output_path);
    if req.sources.is_empty() {
        return Err("at least one source required".into());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut boxes: Vec<Box<dyn HistoricalSource>> = Vec::with_capacity(req.sources.len());
    for src in &req.sources {
        let format = parse_data_format(&src.format);
        let exchange = ExchangeId::new(&src.exchange);
        let symbol = Symbol::new(&src.symbol);
        let path = PathBuf::from(&src.path);
        let opened: Box<dyn HistoricalSource> = match format {
            DataFormat::Auto | DataFormat::Ohlcv => Box::new(
                CsvBarSource::from_path(&path, exchange, symbol).map_err(|e| e.to_string())?,
            ),
            DataFormat::Yahoo => Box::new(
                YahooCsvSource::from_path(&path, exchange, symbol).map_err(|e| e.to_string())?,
            ),
            DataFormat::Fx => Box::new(
                FxCsvSource::from_path(&path, exchange, symbol).map_err(|e| e.to_string())?,
            ),
            DataFormat::Future => Box::new(
                FutureCsvSource::from_path(&path, exchange, symbol).map_err(|e| e.to_string())?,
            ),
        };
        boxes.push(opened);
    }
    write_bar_csv(&output, merge_sources_iter(&mut boxes))?;
    Ok(output.display().to_string())
}

pub fn preview_csv(path: &str) -> Result<CsvPreviewDto, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let header_line = lines
        .next()
        .transpose()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "empty file".to_string())?;
    let headers: Vec<String> = header_line
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let mut head_rows: Vec<Vec<String>> = Vec::new();
    let mut tail_rows: std::collections::VecDeque<Vec<String>> =
        std::collections::VecDeque::with_capacity(5);
    let mut total_rows = 0usize;
    for line in lines {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let row: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
        if head_rows.len() < 5 {
            head_rows.push(row.clone());
        }
        if tail_rows.len() == 5 {
            tail_rows.pop_front();
        }
        tail_rows.push_back(row);
        total_rows += 1;
    }
    Ok(CsvPreviewDto {
        headers,
        head_rows,
        tail_rows: tail_rows.into_iter().collect(),
        total_rows,
    })
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

fn load_rows(path: &Path) -> Result<Vec<ParsedRow>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut rdr = csv::Reader::from_reader(BufReader::new(file));
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
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
            let row = rec.map_err(|e| e.to_string())?;
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
            let row: OhlcvRow = rec.map_err(|e| e.to_string())?;
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

fn write_ohlcv(path: &Path, rows: &[ParsedRow]) -> Result<(), String> {
    let mut w = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
    writeln!(w, "ts,open,high,low,close,volume").map_err(|e| e.to_string())?;
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
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_bar_csv(path: &Path, events: impl IntoIterator<Item = Event>) -> Result<(), String> {
    let mut w = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
    writeln!(w, "ts,exchange,symbol,open,high,low,close,volume").map_err(|e| e.to_string())?;
    for ev in events {
        if let Event::Market(MarketEvent::Bar {
            instrument,
            ts,
            open,
            high,
            low,
            close,
            volume,
            ..
        }) = ev
        {
            writeln!(
                w,
                "{},{},{},{},{},{},{},{}",
                ts.format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| ts.unix_timestamp().to_string()),
                instrument.exchange,
                instrument.symbol,
                open,
                high,
                low,
                close,
                volume
            )
            .map_err(|e| e.to_string())?;
        }
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

fn parse_data_format(s: &str) -> DataFormat {
    match s.to_lowercase().as_str() {
        "ohlcv" | "binance" => DataFormat::Ohlcv,
        "yahoo" => DataFormat::Yahoo,
        "fx" => DataFormat::Fx,
        "future" | "futures" => DataFormat::Future,
        _ => DataFormat::Auto,
    }
}
