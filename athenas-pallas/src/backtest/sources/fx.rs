//! FX CSV with `timestamp,bid,ask` columns.
#![allow(missing_docs)]

use rust_decimal::Decimal;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::backtest::{parse_ts, parse_ts_required_err};
use crate::events::{Event, MarketEvent};
use crate::source::HistoricalSource;
use crate::types::{ExchangeId, InstrumentId, Symbol};

#[derive(Clone, Debug, Deserialize)]
struct FxRow {
    timestamp: String,
    bid: Decimal,
    ask: Decimal,
}

/// FX quote file.
pub struct FxCsvSource {
    rows: Vec<FxRow>,
    idx: usize,
    instrument: InstrumentId,
}

impl FxCsvSource {
    pub fn from_path(path: &Path, exchange: ExchangeId, symbol: Symbol) -> std::io::Result<Self> {
        let mut buf = String::new();
        File::open(path)?.read_to_string(&mut buf)?;
        let mut rdr = csv::Reader::from_reader(buf.as_bytes());
        let mut rows = Vec::new();
        for (i, rec) in rdr.deserialize().enumerate() {
            let row: FxRow =
                rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            parse_ts_required_err(&row.timestamp, &format!("row {}", i + 2))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            rows.push(row);
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

impl HistoricalSource for FxCsvSource {
    fn next_event(&mut self) -> Option<Event> {
        let row = self.rows.get(self.idx)?;
        self.idx += 1;
        let ts = parse_ts(&row.timestamp).expect("timestamp validated at csv load");
        Some(Event::Market(MarketEvent::BookL1 {
            instrument: self.instrument.clone(),
            ts,
            bid: row.bid,
            ask: row.ask,
        }))
    }
}
