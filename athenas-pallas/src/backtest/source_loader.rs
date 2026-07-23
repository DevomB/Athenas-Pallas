//! Historical source loading and CSV format detection.

use std::path::Path;

use super::config::{BacktestConfig, DataFormat};
use super::pbar::is_pbar_path;
use super::sources::FxCsvSource;
use crate::bar::{default_tick_size, BarSeries, BarSeriesSource};
use crate::events::Event;
use crate::source::HistoricalSource;
use crate::types::{ExchangeId, Symbol};

pub(crate) fn load_all_sources(
    cfg: &BacktestConfig,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
    ohlcv_series: Option<BarSeries>,
) -> crate::Result<Vec<Box<dyn HistoricalSource>>> {
    let mut out = vec![load_source(cfg, exchange, symbol, fmt, ohlcv_series)?];
    for extra in &cfg.extra_instruments {
        let Some(path) = &extra.data_path else {
            continue;
        };
        let ex = ExchangeId::new(extra.instrument.exchange.as_str());
        let sym = Symbol::new(extra.instrument.symbol.as_str());
        let fmt = resolve_format(path, extra.data_format.unwrap_or(DataFormat::Auto))?;
        out.push(load_path(path, ex, sym, fmt)?);
    }
    Ok(out)
}

fn load_path(
    path: &Path,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
) -> crate::Result<Box<dyn HistoricalSource>> {
    Ok(match fmt {
        DataFormat::Ohlcv => {
            let series = BarSeries::from_csv_path_or_pbar(path, default_tick_size())
                .map_err(crate::error::Error::Io)?;
            Box::new(BarSeriesSource::new(series, exchange, symbol))
        }
        DataFormat::Fx => Box::new(FxCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Jsonl => Box::new(EventFileSource {
            events: super::read_events_jsonl(std::fs::File::open(path)?)?.into_iter(),
        }),
        DataFormat::Auto => unreachable!(),
    })
}

pub(crate) fn load_source(
    cfg: &BacktestConfig,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
    ohlcv_series: Option<BarSeries>,
) -> crate::Result<Box<dyn HistoricalSource>> {
    if let Some(series) = ohlcv_series {
        debug_assert_eq!(fmt, DataFormat::Ohlcv);
        return Ok(Box::new(BarSeriesSource::new(series, exchange, symbol)));
    }
    load_path(&cfg.data_path, exchange, symbol, fmt)
}

pub(crate) fn resolve_format(path: &Path, configured: DataFormat) -> crate::Result<DataFormat> {
    if configured == DataFormat::Auto {
        detect_format(path)
    } else {
        Ok(configured)
    }
}

pub(crate) fn detect_format(path: &Path) -> crate::Result<DataFormat> {
    if is_pbar_path(path) {
        return Ok(DataFormat::Ohlcv);
    }
    if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
        return Ok(DataFormat::Jsonl);
    }

    let mut rdr = csv::Reader::from_path(path).map_err(|e| {
        crate::error::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;
    let headers = rdr
        .headers()
        .map_err(|e| {
            crate::error::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?
        .clone();
    if headers.iter().any(|h| h == "bid") {
        return Ok(DataFormat::Fx);
    }
    Ok(DataFormat::Ohlcv)
}

struct EventFileSource {
    events: std::vec::IntoIter<Event>,
}

impl HistoricalSource for EventFileSource {
    fn next_event(&mut self) -> Option<Event> {
        self.events.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{MarketDataProvenance, MarketEvent};
    use crate::types::InstrumentId;
    use rust_decimal::Decimal;
    use time::macros::datetime;

    #[test]
    fn jsonl_source_preserves_feed_provenance() {
        let path =
            std::env::temp_dir().join(format!("pallas-events-{}.jsonl", uuid::Uuid::new_v4()));
        let event = Event::Market(MarketEvent::Trade {
            instrument: InstrumentId::new("databento", "ESZ5"),
            ts: datetime!(2025-01-02 14:30 UTC),
            price: Decimal::from(6000),
            qty: Decimal::from(2),
            provenance: Some(MarketDataProvenance {
                dataset: "GLBX.MDP3".into(),
                publisher_id: 1,
                instrument_id: 123,
                ts_recv: Some(datetime!(2025-01-02 14:30:00.000001 UTC)),
                sequence: Some(42),
            }),
        });
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&event).unwrap()),
        )
        .unwrap();
        let mut source = load_path(
            &path,
            ExchangeId::new("ignored"),
            Symbol::new("ignored"),
            DataFormat::Jsonl,
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();

        let Event::Market(MarketEvent::Trade {
            provenance: Some(provenance),
            ..
        }) = source.next_event().unwrap()
        else {
            panic!("expected trade with provenance");
        };
        assert_eq!(provenance.dataset, "GLBX.MDP3");
        assert_eq!(provenance.sequence, Some(42));
    }
}
