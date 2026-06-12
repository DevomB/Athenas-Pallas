//! Futures OHLCV CSV (Yahoo or standard OHLCV columns; contract terms from config).
#![allow(missing_docs)]

use rust_decimal::Decimal;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::backtest::{parse_ts, HistoricalSource, OhlcvRow};
use crate::events::{Event, MarketEvent};
use crate::types::{ExchangeId, InstrumentId, Symbol};

#[derive(Clone, Debug, Deserialize)]
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

enum FutureRow {
    Yahoo(YahooRow),
    Ohlcv(OhlcvRow),
}

/// Futures bar file (same columns as equity Yahoo or OHLCV exports).
pub struct FutureCsvSource {
    rows: Vec<FutureRow>,
    idx: usize,
    instrument: InstrumentId,
}

impl FutureCsvSource {
    pub fn from_path(path: &Path, exchange: ExchangeId, symbol: Symbol) -> std::io::Result<Self> {
        let mut buf = String::new();
        File::open(path)?.read_to_string(&mut buf)?;
        let mut rdr = csv::Reader::from_reader(buf.as_bytes());
        let headers = rdr.headers()?.clone();
        let yahoo = headers.iter().any(|h| h == "Date");
        let mut rows = Vec::new();
        if yahoo {
            for rec in rdr.deserialize::<YahooRow>() {
                rows.push(FutureRow::Yahoo(
                    rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
                ));
            }
        } else {
            for rec in rdr.deserialize::<OhlcvRow>() {
                rows.push(FutureRow::Ohlcv(
                    rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
                ));
            }
        }
        if rows.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "empty csv",
            ));
        }
        Ok(Self {
            rows,
            idx: 0,
            instrument: InstrumentId::new(exchange.to_string(), symbol.to_string()),
        })
    }
}

impl HistoricalSource for FutureCsvSource {
    fn next_event(&mut self) -> Option<Event> {
        let row = self.rows.get(self.idx)?;
        self.idx += 1;
        match row {
            FutureRow::Yahoo(r) => {
                let ts = parse_ts(&r.date).unwrap_or_else(time::OffsetDateTime::now_utc);
                Some(Event::Market(MarketEvent::Bar {
                    instrument: self.instrument.clone(),
                    ts,
                    open: r.open,
                    high: r.high,
                    low: r.low,
                    close: r.close,
                    volume: r.volume,
                }))
            }
            FutureRow::Ohlcv(r) => {
                let ts = parse_ts(&r.ts).unwrap_or_else(time::OffsetDateTime::now_utc);
                Some(Event::Market(MarketEvent::Bar {
                    instrument: self.instrument.clone(),
                    ts,
                    open: r.open,
                    high: r.high,
                    low: r.low,
                    close: r.close,
                    volume: r.volume,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_yahoo_fixture() {
        let dir = std::env::temp_dir().join("pallas_future_csv");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("es.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            "Date,Open,High,Low,Close,Volume\n2024-01-02,4800,4810,4790,4805,100000"
        )
        .unwrap();
        let mut src = FutureCsvSource::from_path(
            &path,
            ExchangeId::new("cme"),
            Symbol::new("ES"),
        )
        .unwrap();
        assert!(src.next_event().is_some());
        assert!(src.next_event().is_none());
    }
}
