//! Historical source loading and CSV format detection.

use std::path::Path;

use super::config::{BacktestConfig, DataFormat};
use super::pbar::is_pbar_path;
use super::sources::FxCsvSource;
use crate::bar::{default_tick_size, BarSeries, BarSeriesSource};
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
