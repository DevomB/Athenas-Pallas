//! Historical source loading and CSV format detection.

use std::path::Path;

use super::bar::{default_tick_size, BarSeries, BarSeriesSource};
use super::config::{BacktestConfig, DataFormat};
use super::pbar::is_pbar_path;
use super::sources::{FutureCsvSource, FxCsvSource, YahooCsvSource};
use super::{CsvBarSource, HistoricalSource};
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
        let fmt = extra.data_format.unwrap_or(DataFormat::Auto);
        let fmt = if fmt == DataFormat::Auto {
            detect_format(path)?
        } else {
            fmt
        };
        out.push(open_source_path(path, ex, sym, fmt)?);
    }
    Ok(out)
}

pub(crate) fn open_source_path(
    path: &Path,
    exchange: ExchangeId,
    symbol: Symbol,
    fmt: DataFormat,
) -> crate::Result<Box<dyn HistoricalSource>> {
    Ok(match fmt {
        DataFormat::Ohlcv => Box::new(CsvBarSource::from_path(path, exchange, symbol)?),
        DataFormat::Yahoo => Box::new(YahooCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Fx => Box::new(FxCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Future => Box::new(FutureCsvSource::from_path(path, exchange, symbol)?),
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
    let path = &cfg.data_path;
    Ok(match fmt {
        DataFormat::Ohlcv => {
            if let Some(series) = ohlcv_series {
                Box::new(BarSeriesSource::new(series, exchange, symbol))
            } else if is_pbar_path(path) {
                let series = BarSeries::from_csv_path_or_pbar(path, default_tick_size())
                    .map_err(crate::error::Error::Io)?;
                Box::new(BarSeriesSource::new(series, exchange, symbol))
            } else {
                Box::new(CsvBarSource::from_path(path, exchange, symbol)?)
            }
        }
        DataFormat::Yahoo => Box::new(YahooCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Fx => Box::new(FxCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Future => Box::new(FutureCsvSource::from_path(path, exchange, symbol)?),
        DataFormat::Auto => unreachable!(),
    })
}

pub(crate) fn detect_format(path: &Path) -> crate::Result<DataFormat> {
    use super::sources::headers_are_yahoo;

    let mut rdr = csv::Reader::from_path(path).map_err(|e| {
        crate::error::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;
    let headers = rdr
        .headers()
        .map_err(|e| {
            crate::error::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?
        .clone();
    if headers_are_yahoo(&headers) {
        return Ok(DataFormat::Yahoo);
    }
    if headers.iter().any(|h| h == "bid") {
        return Ok(DataFormat::Fx);
    }
    Ok(DataFormat::Ohlcv)
}
