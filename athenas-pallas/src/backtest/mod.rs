//! Historical and synthetic event sources for deterministic replay.

pub mod bar;
pub mod batch;
pub mod config;
pub mod cpp_build;
pub mod interval;
pub mod lifecycle;
pub mod merge;
pub mod pbar;
pub mod replay;
pub mod runner;
pub mod session;
pub mod sources;
pub mod strategy_resolver;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rust_decimal::Decimal;
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, PrimitiveDateTime};

use crate::events::{Event, MarketEvent};
use crate::types::{ExchangeId, InstrumentId, Symbol};

/// Produces timestamped [`Event`] values.
pub trait HistoricalSource: Send {
    /// Next event in order.
    fn next_event(&mut self) -> Option<Event>;
}

/// OHLCV row for [`CsvBarSource`].
#[derive(Clone, Debug, Deserialize)]
pub struct OhlcvRow {
    /// Timestamp string (RFC3339 or `YYYY-MM-DD HH:MM:SS` or date-only).
    pub ts: String,
    /// Open.
    pub open: Decimal,
    /// High.
    pub high: Decimal,
    /// Low.
    pub low: Decimal,
    /// Close.
    pub close: Decimal,
    /// Volume.
    pub volume: Decimal,
}

/// CSV bar replay (`ts,open,high,low,close,volume` header).
pub struct CsvBarSource {
    rows: Vec<OhlcvRow>,
    idx: usize,
    instrument: InstrumentId,
}

impl CsvBarSource {
    /// Load from disk.
    pub fn from_path(path: &Path, exchange: ExchangeId, symbol: Symbol) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(BufReader::new(file));
        let mut rows = Vec::new();
        for (i, rec) in rdr.deserialize().enumerate() {
            let row: OhlcvRow =
                rec.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            parse_ts_required(&row.ts, &format!("row {}", i + 2))?;
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

    /// Parse UTF-8 CSV text.
    pub fn from_str(data: &str, exchange: ExchangeId, symbol: Symbol) -> Result<Self, String> {
        let mut rdr = csv::Reader::from_reader(data.as_bytes());
        let mut rows = Vec::new();
        for (i, rec) in rdr.deserialize().enumerate() {
            let row: OhlcvRow = rec.map_err(|e| e.to_string())?;
            parse_ts_required_err(&row.ts, &format!("row {}", i + 2))?;
            rows.push(row);
        }
        if rows.is_empty() {
            return Err("empty csv".into());
        }
        Ok(Self {
            rows,
            idx: 0,
            instrument: InstrumentId::new(exchange.to_string(), symbol.to_string()),
        })
    }

    /// In-memory rows (already parsed).
    pub fn from_ohlcv_rows(exchange: ExchangeId, symbol: Symbol, rows: Vec<OhlcvRow>) -> Self {
        Self {
            rows,
            idx: 0,
            instrument: InstrumentId::new(exchange.to_string(), symbol.to_string()),
        }
    }
}

pub(crate) fn parse_ts(s: &str) -> Option<OffsetDateTime> {
    let s = s.trim();
    if let Ok(t) = OffsetDateTime::parse(s, &Rfc3339) {
        return Some(t);
    }
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    if let Ok(p) = PrimitiveDateTime::parse(s, &fmt) {
        return Some(p.assume_utc());
    }
    use time::Date;
    let fmt2 = format_description!("[year]-[month]-[day]");
    if let Ok(d) = Date::parse(s, &fmt2) {
        return d.with_hms(0, 0, 0).ok().map(|dt| dt.assume_utc());
    }
    None
}

pub(crate) fn parse_ts_required(s: &str, context: &str) -> std::io::Result<OffsetDateTime> {
    parse_ts(s).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid timestamp {context}: {s}"),
        )
    })
}

pub(crate) fn parse_ts_required_err(s: &str, context: &str) -> Result<OffsetDateTime, String> {
    parse_ts(s).ok_or_else(|| format!("invalid timestamp {context}: {s}"))
}

/// Parse RFC3339 or common CSV timestamp formats (public wrapper for binaries).
pub fn parse_timestamp(s: &str) -> Option<OffsetDateTime> {
    parse_ts(s)
}

impl HistoricalSource for CsvBarSource {
    fn next_event(&mut self) -> Option<Event> {
        let row = self.rows.get(self.idx)?;
        self.idx += 1;
        let ts = parse_ts(&row.ts).expect("timestamp validated at csv load");
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

/// Deterministic toy random walk for smoke tests.
pub struct SyntheticGmSource {
    steps: usize,
    taken: usize,
    price: Decimal,
    instrument: InstrumentId,
    start: OffsetDateTime,
    drift: Decimal,
    vol: Decimal,
    seed: u64,
}

impl SyntheticGmSource {
    /// Multiplicative steps around `drift` with noise scaled by `vol`.
    pub fn new(
        instrument: InstrumentId,
        start: OffsetDateTime,
        start_price: Decimal,
        steps: usize,
        drift: Decimal,
        vol: Decimal,
        seed: u64,
    ) -> Self {
        Self {
            steps,
            taken: 0,
            price: start_price,
            instrument,
            start,
            drift,
            vol,
            seed,
        }
    }

    fn rng_next(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed as f64) / (u64::MAX as f64)
    }
}

impl HistoricalSource for SyntheticGmSource {
    fn next_event(&mut self) -> Option<Event> {
        if self.taken >= self.steps {
            return None;
        }
        self.taken += 1;
        let u = self.rng_next() - 0.5;
        let shock = Decimal::new((u * 1_000_000.0).round() as i64, 6) * self.vol;
        self.price = (self.price * (self.drift + shock)).max(Decimal::new(1, 2));
        let ts = self.start + Duration::seconds(self.taken as i64);
        let pad = self.price / Decimal::from(2000u64);
        Some(Event::Market(MarketEvent::BookL1 {
            instrument: self.instrument.clone(),
            ts,
            bid: self.price - pad,
            ask: self.price + pad,
        }))
    }
}

/// OHLC row without volume (embedded example / tight CSV).
#[derive(Clone, Debug, Deserialize)]
pub struct OhlcRow {
    /// Timestamp (RFC3339 recommended).
    pub ts: String,
    /// Open.
    pub open: Decimal,
    /// High.
    pub high: Decimal,
    /// Low.
    pub low: Decimal,
    /// Close.
    pub close: Decimal,
}

/// In-memory OHLC replay as synthetic [`MarketEvent::BookL1`].
pub struct CsvOhlcSource {
    instrument: InstrumentId,
    rows: Vec<OhlcRow>,
    idx: usize,
    half_spread_from_mid_bps: Decimal,
}

impl CsvOhlcSource {
    /// Iterate owned rows.
    pub fn from_rows(instrument: InstrumentId, rows: Vec<OhlcRow>, spread_bps: Decimal) -> Self {
        Self {
            instrument,
            rows,
            idx: 0,
            half_spread_from_mid_bps: spread_bps / Decimal::from(2u64),
        }
    }
}

impl HistoricalSource for CsvOhlcSource {
    fn next_event(&mut self) -> Option<Event> {
        let row = self.rows.get(self.idx)?;
        self.idx += 1;
        let ts = parse_ts(&row.ts).expect("timestamp validated at csv load");
        let mid = row.close;
        let half_spread = mid * self.half_spread_from_mid_bps / Decimal::from(10_000u64);
        let bid = mid - half_spread;
        let ask = mid + half_spread;
        Some(Event::Market(MarketEvent::BookL1 {
            instrument: self.instrument.clone(),
            ts,
            bid,
            ask,
        }))
    }
}

/// Optional hook for custom microstructure models.
pub trait FillModel: Send + Sync {
    /// Label for logging.
    fn name(&self) -> &'static str;

    /// True when a resting limit would cross the touch (paper/sim use this).
    fn limit_would_fill(
        &self,
        side: crate::types::Side,
        limit: Decimal,
        bid: Decimal,
        ask: Decimal,
    ) -> bool;
}

/// Default touch-based fill assumption (fills use paper/sim gateways).
#[derive(Default)]
pub struct TouchCrossFillModel;

impl FillModel for TouchCrossFillModel {
    fn name(&self) -> &'static str {
        "touch_cross"
    }

    fn limit_would_fill(
        &self,
        side: crate::types::Side,
        limit: Decimal,
        bid: Decimal,
        ask: Decimal,
    ) -> bool {
        match side {
            crate::types::Side::Buy => limit >= ask,
            crate::types::Side::Sell => limit <= bid,
        }
    }
}

pub use bar::{
    decimal_to_ticks, default_tick_size, ticks_to_decimal, Bar, BarSeries, BarSeriesSource,
};
pub use batch::{
    run_scenarios_parallel, run_scenarios_parallel_sync, run_scenarios_serial, RunReport, Scenario,
};
pub use config::{parse_base_quote, parse_instrument, BacktestConfig, DataFormat, ExtraInstrument};
pub use cpp_build::build_cpp_strategy;
pub use interval::{
    default_periods_per_year, infer_periods_per_year_from_spacing,
    infer_periods_per_year_from_timestamps, periods_per_year_from_interval,
    periods_per_year_from_interval_for_class,
};
pub use merge::{merge_sources, merge_sources_iter, MergedSources};
pub use pbar::{is_pbar_path, read_pbar, write_pbar};
pub use replay::{read_events_jsonl, replay_events_serial};
pub use runner::BuyAndHold;
pub use runner::{BacktestReport, BacktestRunner};
pub use session::{
    downsample_equity, report_to_dto, run_backtest, run_backtest_with_cancel,
    run_external_backtest, run_external_backtest_with_cancel, BacktestReportDto, EquityPointDto,
};
pub use strategy_resolver::{detect_strategy, resolve_strategy_path, ResolvedStrategy};

#[cfg(test)]
mod csv_tests {
    use super::*;
    use crate::events::Event;
    use rust_decimal::Decimal;
    use std::path::PathBuf;

    fn sample_csv() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/BTCUSDT_1d.csv")
    }

    #[test]
    fn from_path_yields_ninety_events() {
        let mut src = CsvBarSource::from_path(
            &sample_csv(),
            ExchangeId("binance".into()),
            Symbol("BTCUSDT".into()),
        )
        .expect("csv");
        let mut n = 0;
        while src.next_event().is_some() {
            n += 1;
        }
        assert_eq!(n, 90);
    }

    #[test]
    fn from_path_last_close() {
        let mut src = CsvBarSource::from_path(
            &sample_csv(),
            ExchangeId("binance".into()),
            Symbol("BTCUSDT".into()),
        )
        .expect("csv");
        let mut last = None;
        while let Some(ev) = src.next_event() {
            last = Some(ev);
        }
        let Event::Market(MarketEvent::Bar { close, .. }) = last.unwrap() else {
            panic!("expected Bar");
        };
        assert!(close > Decimal::new(44_000, 0));
    }

    #[test]
    fn empty_csv_errors() {
        let res = CsvBarSource::from_str(
            "ts,open,high,low,close,volume\n",
            ExchangeId("x".into()),
            Symbol("Y".into()),
        );
        assert!(matches!(res, Err(ref e) if e.contains("empty csv")));
    }
}
