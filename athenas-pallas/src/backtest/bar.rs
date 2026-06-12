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

impl Bar {
    pub fn open_decimal(&self, tick_size: Decimal) -> Decimal {
        ticks_to_decimal(self.open_ticks, tick_size)
    }

    pub fn high_decimal(&self, tick_size: Decimal) -> Decimal {
        ticks_to_decimal(self.high_ticks, tick_size)
    }

    pub fn low_decimal(&self, tick_size: Decimal) -> Decimal {
        ticks_to_decimal(self.low_ticks, tick_size)
    }

    pub fn close_decimal(&self, tick_size: Decimal) -> Decimal {
        ticks_to_decimal(self.close_ticks, tick_size)
    }

    pub fn volume_decimal(&self, tick_size: Decimal) -> Decimal {
        ticks_to_decimal(self.volume_lots, tick_size)
    }

    pub fn timestamp(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp_nanos(self.ts_unix_nanos as i128).ok()
    }
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
        let mut bars = Vec::with_capacity(buf.lines().count().saturating_sub(1));
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
    exchange: ExchangeId,
    symbol: Symbol,
}

impl BarSeriesSource {
    pub fn new(series: BarSeries, exchange: ExchangeId, symbol: Symbol) -> Self {
        Self {
            series,
            idx: 0,
            exchange,
            symbol,
        }
    }

    pub fn tick_size(&self) -> Decimal {
        self.series.tick_size()
    }

    pub fn instrument_id(&self) -> InstrumentId {
        InstrumentId::new(self.exchange.to_string(), self.symbol.to_string())
    }

    pub fn rewind(&mut self) {
        self.idx = 0;
    }

    /// Next bar without allocating a market event.
    pub fn next_bar(&mut self) -> Option<(Bar, OffsetDateTime)> {
        let bar = *self.series.get(self.idx)?;
        self.idx += 1;
        let ts = bar.timestamp()?;
        Some((bar, ts))
    }

    pub fn bar_to_event(&self, bar: &Bar, ts: OffsetDateTime) -> Event {
        let tick = self.series.tick_size;
        Event::Market(MarketEvent::Bar {
            instrument: self.instrument_id(),
            ts,
            open: bar.open_decimal(tick),
            high: bar.high_decimal(tick),
            low: bar.low_decimal(tick),
            close: bar.close_decimal(tick),
            volume: bar.volume_decimal(tick),
        })
    }
}

impl super::HistoricalSource for BarSeriesSource {
    fn next_event(&mut self) -> Option<Event> {
        let (bar, ts) = self.next_bar()?;
        Some(self.bar_to_event(&bar, ts))
    }
}

pub fn decimal_to_ticks(d: Decimal, tick_size: Decimal) -> i64 {
    if tick_size.is_zero() {
        return 0;
    }
    let scaled = d / tick_size;
    scaled
        .round()
        .mantissa()
        .try_into()
        .unwrap_or(i64::MAX)
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
            .join("tests/fixtures/data/BTCUSDT_1d.csv");
        let s = BarSeries::from_csv_path(&path, default_tick_size()).expect("csv");
        assert_eq!(s.len(), 90);
    }
}
