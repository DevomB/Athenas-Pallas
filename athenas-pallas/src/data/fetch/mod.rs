//! Download historical OHLCV from public APIs.
#![allow(missing_docs)]

pub mod binance;
pub mod intervals;
pub mod yahoo;

use crate::error::{Error, Result};
use rust_decimal::Decimal;
use std::path::Path;
use time::OffsetDateTime;

/// One OHLCV row in engine CSV format.
#[derive(Clone, Debug)]
pub struct OhlcvBar {
    pub ts: OffsetDateTime,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

/// Yahoo-style daily row.
#[derive(Clone, Debug)]
pub struct YahooBar {
    pub date: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    /// Split/dividend-adjusted close when available from API.
    pub adj_close: Option<Decimal>,
    pub volume: Decimal,
}

/// Write `ts,open,high,low,close,volume` CSV.
pub fn write_ohlcv_csv(path: &Path, bars: &[OhlcvBar]) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path).map_err(|e| Error::Io(e.into()))?;
    for b in bars {
        wtr.write_record([
            b.ts.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            b.open.to_string(),
            b.high.to_string(),
            b.low.to_string(),
            b.close.to_string(),
            b.volume.to_string(),
        ])
        .map_err(|e| Error::Io(e.into()))?;
    }
    wtr.flush().map_err(|e| Error::Io(e.into()))?;
    Ok(())
}

/// Write Yahoo `Date,Open,High,Low,Close,Adj Close,Volume` CSV.
pub fn write_yahoo_csv(path: &Path, bars: &[YahooBar]) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path).map_err(|e| Error::Io(e.into()))?;
    let has_adj = bars.iter().any(|b| b.adj_close.is_some());
    if has_adj {
        wtr.write_record([
            "Date",
            "Open",
            "High",
            "Low",
            "Close",
            "Adj Close",
            "Volume",
        ])
        .map_err(|e| Error::Io(e.into()))?;
    } else {
        wtr.write_record(["Date", "Open", "High", "Low", "Close", "Volume"])
            .map_err(|e| Error::Io(e.into()))?;
    }
    for b in bars {
        if has_adj {
            wtr.write_record([
                b.date.clone(),
                b.open.to_string(),
                b.high.to_string(),
                b.low.to_string(),
                b.close.to_string(),
                b.adj_close.unwrap_or(b.close).to_string(),
                b.volume.to_string(),
            ])
            .map_err(|e| Error::Io(e.into()))?;
        } else {
            wtr.write_record([
                b.date.clone(),
                b.open.to_string(),
                b.high.to_string(),
                b.low.to_string(),
                b.close.to_string(),
                b.volume.to_string(),
            ])
            .map_err(|e| Error::Io(e.into()))?;
        }
    }
    wtr.flush().map_err(|e| Error::Io(e.into()))?;
    Ok(())
}
