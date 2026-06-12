//! OHLC bars stored as fixed-width ticks for cache-friendly replay.
#![allow(missing_docs)]

use rust_decimal::Decimal;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use time::OffsetDateTime;

use super::{parse_ts, OhlcvRow};
use crate::events::{Event, MarketEvent};
use crate::types::{ExchangeId, InstrumentId, Symbol};

/// Default tick size: 1e-8 quote units per tick.
pub fn default_tick_size() -> Decimal {
    Decimal::new(1, 8)
}

/// One bar in fixed-point ticks.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub ts_unix_nanos: i64,
    pub open_ticks: i64,
    pub high_ticks: i64,
    pub low_ticks: i64,
    pub close_ticks: i64,
    pub volume_lots: i64,
}

/// Contiguous bar store — one allocation at load time.
#[derive(Clone, Debug)]
pub struct BarSeries {
    bars: Vec<Bar>,
    tick_size: Decimal,
}

impl BarSeries {
    /// Load OHLCV CSV (`ts,open,high,low,close,volume`).
    pub fn from_csv_path(path: &Path, tick_size: Decimal) -> std::io::Result<Self> {
        let mut buf = String::new();
        File::open(path)?.read_to_string(&mut buf)?;
        let mut rdr = csv::Reader::from_reader(buf.as_bytes());
        let mut bars = Vec::new();
        for rec in rdr.deserialize::<OhlcvRow>() {
            let row: OhlcvRow = rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let ts = parse_ts(&row.ts).unwrap_or_else(time::OffsetDateTime::now_utc);
            bars.push(Bar {
                ts_unix_nanos: ts.unix_timestamp_nanos() as i64,
                open_ticks: decimal_to_ticks(row.open, tick_size),
                high_ticks: decimal_to_ticks(row.high, tick_size),
                low_ticks: decimal_to_ticks(row.low, tick_size),
                close_ticks: decimal_to_ticks(row.close, tick_size),
                volume_lots: decimal_to_ticks(row.volume, tick_size),
            });
        }
        if bars.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "empty csv",
            ));
        }
        Ok(Self { bars, tick_size })
    }

    /// Deterministic random walk for benchmarks.
    pub fn random_walk(n: usize, seed: u64, start_price: Decimal, tick_size: Decimal) -> Self {
        let start = decimal_to_ticks(start_price, tick_size);
        let mut bars = Vec::with_capacity(n);
        let mut price = start;
        let mut s = seed;
        let base_ts = time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        for i in 0..n {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let shock = ((s % 200) as i64) - 100;
            price = (price + shock).max(1);
            let ts = base_ts + (i as i64) * 86_400_000_000_000;
            bars.push(Bar {
                ts_unix_nanos: ts,
                open_ticks: price,
                high_ticks: price + 50,
                low_ticks: price - 50,
                close_ticks: price,
                volume_lots: 1,
            });
        }
        Self { bars, tick_size }
    }

    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&Bar> {
        self.bars.get(i)
    }

    pub fn tick_size(&self) -> Decimal {
        self.tick_size
    }

    pub fn close_decimal(&self, i: usize) -> Option<Decimal> {
        self.get(i).map(|b| ticks_to_decimal(b.close_ticks, self.tick_size))
    }
}

/// Walk a preloaded series by index.
pub struct BarSeriesSource {
    series: BarSeries,
    idx: usize,
    instrument: InstrumentId,
}

impl BarSeriesSource {
    pub fn new(series: BarSeries, exchange: ExchangeId, symbol: Symbol) -> Self {
        Self {
            series,
            idx: 0,
            instrument: InstrumentId::new(exchange.to_string(), symbol.to_string()),
        }
    }
}

impl super::HistoricalSource for BarSeriesSource {
    fn next_event(&mut self) -> Option<Event> {
        let bar = self.series.get(self.idx)?;
        self.idx += 1;
        let ts = OffsetDateTime::from_unix_timestamp_nanos(bar.ts_unix_nanos as i128).ok()?;
        let tick = self.series.tick_size;
        Some(Event::Market(MarketEvent::Bar {
            instrument: self.instrument.clone(),
            ts,
            open: ticks_to_decimal(bar.open_ticks, tick),
            high: ticks_to_decimal(bar.high_ticks, tick),
            low: ticks_to_decimal(bar.low_ticks, tick),
            close: ticks_to_decimal(bar.close_ticks, tick),
            volume: ticks_to_decimal(bar.volume_lots, tick),
        }))
    }
}

pub fn decimal_to_ticks(d: Decimal, tick_size: Decimal) -> i64 {
    if tick_size.is_zero() {
        return 0;
    }
    (d / tick_size)
        .round()
        .to_string()
        .parse::<i64>()
        .unwrap_or(0)
}

pub fn ticks_to_decimal(t: i64, tick_size: Decimal) -> Decimal {
    Decimal::from(t) * tick_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn round_trip_ticks() {
        let tick = default_tick_size();
        let d = Decimal::new(40_000, 0);
        let t = decimal_to_ticks(d, tick);
        let back = ticks_to_decimal(t, tick);
        assert_eq!(back, d);
    }

    #[test]
    fn csv_ninety_bars() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("data")
            .join("BTCUSDT_1d.csv");
        let s = BarSeries::from_csv_path(&path, default_tick_size()).expect("csv");
        assert_eq!(s.len(), 90);
    }
}
