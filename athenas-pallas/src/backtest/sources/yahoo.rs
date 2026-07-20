//! Yahoo Finance daily CSV (`Date,Open,High,Low,Close,Adj Close,Volume`).
#![allow(missing_docs)]

use std::path::Path;

use super::csv_common::{parse_yahoo_csv, YahooRow};
use crate::backtest::parse_ts;
use crate::events::{Event, MarketEvent};
use crate::source::HistoricalSource;
use crate::types::{ExchangeId, InstrumentId, Symbol};

/// Yahoo-format OHLCV file.
pub struct YahooCsvSource {
    rows: Vec<YahooRow>,
    idx: usize,
    instrument: InstrumentId,
}

impl YahooCsvSource {
    pub fn from_path(path: &Path, exchange: ExchangeId, symbol: Symbol) -> std::io::Result<Self> {
        let rows = parse_yahoo_csv(path)?;
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
        let ts = parse_ts(&row.date).expect("timestamp validated at csv load");
        let close = row.effective_close();
        Some(Event::Market(MarketEvent::Bar {
            instrument: self.instrument.clone(),
            ts,
            open: row.open,
            high: row.high,
            low: row.low,
            close,
            volume: row.volume,
        }))
    }
}
