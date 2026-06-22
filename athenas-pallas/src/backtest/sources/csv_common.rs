//! Shared Yahoo-format CSV parsing.

use rust_decimal::Decimal;
use serde::Deserialize;
use std::io::{self, Read};
use std::path::Path;

use crate::backtest::parse_ts_required_err;

/// One row from a Yahoo Finance daily export.
#[derive(Clone, Debug, Deserialize)]
pub struct YahooRow {
    /// Date column.
    #[serde(rename = "Date")]
    pub date: String,
    /// Open price.
    #[serde(rename = "Open")]
    pub open: Decimal,
    /// High price.
    #[serde(rename = "High")]
    pub high: Decimal,
    /// Low price.
    #[serde(rename = "Low")]
    pub low: Decimal,
    /// Close price.
    #[serde(rename = "Close")]
    pub close: Decimal,
    /// Adjusted close when present.
    #[serde(rename = "Adj Close", default)]
    pub adj_close: Option<Decimal>,
    /// Volume.
    #[serde(rename = "Volume")]
    pub volume: Decimal,
}

impl YahooRow {
    /// Close price used for replay (prefers adjusted close).
    pub fn effective_close(&self) -> Decimal {
        self.adj_close.unwrap_or(self.close)
    }
}

/// True when CSV headers indicate Yahoo layout (`Date` column).
pub fn headers_are_yahoo(headers: &csv::StringRecord) -> bool {
    headers.iter().any(|h| h == "Date")
}

/// Load and validate Yahoo rows from a file path.
pub fn parse_yahoo_csv(path: &Path) -> io::Result<Vec<YahooRow>> {
    let mut buf = String::new();
    std::fs::File::open(path)?.read_to_string(&mut buf)?;
    let mut rdr = csv::Reader::from_reader(buf.as_bytes());
    read_yahoo_rows(&mut rdr)
}

/// Deserialize Yahoo rows from an existing CSV reader (after headers).
pub fn read_yahoo_rows<R: std::io::Read>(rdr: &mut csv::Reader<R>) -> io::Result<Vec<YahooRow>> {
    let mut rows = Vec::new();
    for (i, rec) in rdr.deserialize().enumerate() {
        let row: YahooRow = rec.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        parse_ts_required_err(&row.date, &format!("row {}", i + 2))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "empty csv"));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_adj_close_when_present() {
        let csv = "Date,Open,High,Low,Close,Adj Close,Volume\n2024-01-02,10,11,9,10,9.5,100\n";
        let mut rdr = csv::Reader::from_reader(csv.as_bytes());
        let rows = read_yahoo_rows(&mut rdr).unwrap();
        assert_eq!(rows[0].effective_close(), Decimal::new(95, 1));
    }
}
