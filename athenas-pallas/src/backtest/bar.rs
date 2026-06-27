//! OHLC bars stored as fixed-width ticks for cache-friendly replay.
#![allow(missing_docs)]

use rust_decimal::Decimal;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use time::OffsetDateTime;

use super::{parse_ts_required, OhlcvRow};
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
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(self.ts_unix_nanos)).ok()
    }
}

/// Contiguous bar store - one allocation at load time.
#[derive(Clone, Debug)]
pub struct BarSeries {
    bars: Vec<Bar>,
    tick_size: Decimal,
}

impl BarSeries {
    /// Build from pre-encoded bars (e.g. after reading `.pbar`).
    pub fn from_bars(bars: Vec<Bar>, tick_size: Decimal) -> Self {
        Self { bars, tick_size }
    }

    /// Load OHLCV CSV (`ts,open,high,low,close,volume`).
    pub fn from_csv_path(path: &Path, tick_size: Decimal) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(BufReader::new(file));
        let mut bars = Vec::new();
        for (i, rec) in rdr.deserialize::<OhlcvRow>().enumerate() {
            let row: OhlcvRow =
                rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let ts = parse_ts_required(&row.ts, &format!("row {}", i + 2))?;
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
        let mut price = start;
        let mut s = seed;
        let base_ts = time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64;
        let bars = (0..n)
            .map(|i| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let shock = i64::try_from(s % 200).unwrap_or_default() - 100;
                price = (price + shock).max(1);
                let ts = base_ts
                    + i64::try_from(i).unwrap_or(i64::MAX / 86_400_000_000_000)
                        * 86_400_000_000_000;
                Bar {
                    ts_unix_nanos: ts,
                    open_ticks: price,
                    high_ticks: price + 50,
                    low_ticks: price - 50,
                    close_ticks: price,
                    volume_lots: 1,
                }
            })
            .collect();
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

    pub fn iter(&self) -> impl Iterator<Item = &Bar> {
        self.bars.iter()
    }

    pub fn tick_size(&self) -> Decimal {
        self.tick_size
    }

    pub fn close_decimal(&self, i: usize) -> Option<Decimal> {
        self.get(i)
            .map(|b| ticks_to_decimal(b.close_ticks, self.tick_size))
    }

    /// Infer periods-per-year from median bar spacing, walking `ts_unix_nanos` directly.
    ///
    /// Avoids the `Vec<OffsetDateTime>` (and per-bar `OffsetDateTime` conversions) that the old
    /// `resolve_periods_per_year` path allocated at startup. Deltas are computed in integer
    /// seconds; only a `Vec<i64>` of spacings is needed for the median.
    pub fn infer_periods_per_year(&self, class: crate::instrument::AssetClass) -> f64 {
        use super::interval::{default_periods_per_year, infer_periods_per_year_from_spacing};
        if self.bars.len() < 2 {
            return default_periods_per_year(class);
        }
        let mut deltas: Vec<i64> = Vec::with_capacity(self.bars.len() - 1);
        for w in self.bars.windows(2) {
            let secs = (w[1].ts_unix_nanos - w[0].ts_unix_nanos) / 1_000_000_000;
            if secs > 0 {
                deltas.push(secs);
            }
        }
        if deltas.is_empty() {
            return default_periods_per_year(class);
        }
        deltas.sort_unstable();
        let median = deltas[deltas.len() / 2] as f64;
        infer_periods_per_year_from_spacing(median, class)
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
        let instrument = InstrumentId::new(exchange.to_string(), symbol.to_string());
        Self {
            series,
            idx: 0,
            instrument,
        }
    }

    pub fn tick_size(&self) -> Decimal {
        self.series.tick_size()
    }

    pub fn instrument_id(&self) -> InstrumentId {
        self.instrument.clone()
    }

    /// Borrowed [`ReplayEvent`] for the zero-allocation fast path (no per-bar `InstrumentId` clone).
    pub fn bar_to_replay_event<'a>(
        &'a self,
        bar: &Bar,
        ts: OffsetDateTime,
    ) -> crate::events::ReplayEvent<'a> {
        let tick = self.series.tick_size;
        crate::events::ReplayEvent::Bar {
            instrument: &self.instrument,
            ts,
            open: bar.open_decimal(tick),
            high: bar.high_decimal(tick),
            low: bar.low_decimal(tick),
            close: bar.close_decimal(tick),
            volume: bar.volume_decimal(tick),
        }
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
    scaled.round().mantissa().try_into().unwrap_or(i64::MAX)
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
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/BTCUSDT_1d.csv");
        let s = BarSeries::from_csv_path(&path, default_tick_size()).expect("csv");
        assert_eq!(s.len(), 90);
    }
}
