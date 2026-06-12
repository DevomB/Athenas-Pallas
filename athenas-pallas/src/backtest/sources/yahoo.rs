//! Yahoo Finance daily CSV (`Date,Open,High,Low,Close,Adj Close,Volume`).
#![allow(missing_docs)]

use rust_decimal::Decimal;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::backtest::{parse_ts, HistoricalSource};
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

/// Yahoo-format OHLCV file.
pub struct YahooCsvSource {
    rows: Vec<YahooRow>,
    idx: usize,
    instrument: InstrumentId,
}

impl YahooCsvSource {
    pub fn from_path(path: &Path, exchange: ExchangeId, symbol: Symbol) -> std::io::Result<Self> {
        let mut buf = String::new();
        File::open(path)?.read_to_string(&mut buf)?;
        let mut rdr = csv::Reader::from_reader(buf.as_bytes());
        let mut rows = Vec::new();
        for rec in rdr.deserialize() {
            rows.push(rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?);
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

impl HistoricalSource for YahooCsvSource {
    fn next_event(&mut self) -> Option<Event> {
        let row = self.rows.get(self.idx)?;
        self.idx += 1;
        let ts = parse_ts(&row.date).unwrap_or_else(time::OffsetDateTime::now_utc);
        Some(Event::Market(MarketEvent::Bar {
            instrument: self.instrument.clone(),
            ts,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
        }))
    }
}
