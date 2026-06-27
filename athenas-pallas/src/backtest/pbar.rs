//! Binary bar cache (`.pbar`) for fast replay without CSV parsing.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use rust_decimal::Decimal;

use super::bar::{Bar, BarSeries};

const MAGIC: &[u8; 4] = b"PBAR";
const VERSION: u32 = 1;
const BAR_BYTES: usize = 6 * std::mem::size_of::<i64>();

impl BarSeries {
    /// Load OHLCV CSV (`ts,open,high,low,close,volume`) or binary `.pbar` cache.
    pub fn from_csv_path_or_pbar(path: &Path, tick_size: Decimal) -> std::io::Result<Self> {
        if is_pbar_path(path) {
            return read_pbar(path);
        }
        let sidecar = path.with_extension("pbar");
        if sidecar_is_fresh(path, &sidecar) {
            if let Ok(series) = read_pbar(&sidecar) {
                return Ok(series);
            }
        }
        let series = Self::from_csv_path(path, tick_size)?;
        let _ = write_pbar(&sidecar, &series);
        Ok(series)
    }
}

/// Write a [`BarSeries`] to a `.pbar` file.
pub fn write_pbar(path: &Path, series: &BarSeries) -> std::io::Result<()> {
    let mut f = BufWriter::new(File::create(path)?);
    f.write_all(MAGIC)?;
    f.write_all(&VERSION.to_le_bytes())?;
    let tick = series.tick_size();
    let tick_str = tick.to_string();
    let tick_len = tick_str.len() as u32;
    f.write_all(&tick_len.to_le_bytes())?;
    f.write_all(tick_str.as_bytes())?;
    let count = series.len() as u64;
    f.write_all(&count.to_le_bytes())?;
    for bar in series.iter() {
        f.write_all(&bar.ts_unix_nanos.to_le_bytes())?;
        f.write_all(&bar.open_ticks.to_le_bytes())?;
        f.write_all(&bar.high_ticks.to_le_bytes())?;
        f.write_all(&bar.low_ticks.to_le_bytes())?;
        f.write_all(&bar.close_ticks.to_le_bytes())?;
        f.write_all(&bar.volume_lots.to_le_bytes())?;
    }
    Ok(())
}

/// Read a `.pbar` file into a [`BarSeries`].
pub fn read_pbar(path: &Path) -> std::io::Result<BarSeries> {
    let mut f = File::open(path)?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid pbar magic",
        ));
    }
    let mut ver_buf = [0u8; 4];
    f.read_exact(&mut ver_buf)?;
    let version = u32::from_le_bytes(ver_buf);
    if version != VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported pbar version {version}"),
        ));
    }
    let mut len_buf = [0u8; 4];
    f.read_exact(&mut len_buf)?;
    let tick_len = u32::from_le_bytes(len_buf) as usize;
    let mut tick_bytes = vec![0u8; tick_len];
    f.read_exact(&mut tick_bytes)?;
    let tick_str = std::str::from_utf8(&tick_bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tick_size: Decimal = tick_str.parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid tick_size: {e}"),
        )
    })?;
    let mut count_buf = [0u8; 8];
    f.read_exact(&mut count_buf)?;
    let count = u64::from_le_bytes(count_buf) as usize;
    let expected = count.checked_mul(BAR_BYTES).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "pbar row count overflow")
    })?;
    let mut payload = Vec::with_capacity(expected);
    f.read_to_end(&mut payload)?;
    if payload.len() != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!("pbar payload length {} expected {expected}", payload.len()),
        ));
    }

    let mut bars = Vec::with_capacity(count);
    for chunk in payload.chunks_exact(BAR_BYTES) {
        bars.push(Bar {
            ts_unix_nanos: read_i64(&chunk[0..8]),
            open_ticks: read_i64(&chunk[8..16]),
            high_ticks: read_i64(&chunk[16..24]),
            low_ticks: read_i64(&chunk[24..32]),
            close_ticks: read_i64(&chunk[32..40]),
            volume_lots: read_i64(&chunk[40..48]),
        });
    }
    if bars.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "empty pbar",
        ));
    }
    Ok(BarSeries::from_bars(bars, tick_size))
}

fn read_i64(bytes: &[u8]) -> i64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(bytes);
    i64::from_le_bytes(buf)
}

/// True when path looks like a `.pbar` cache file.
pub fn is_pbar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pbar"))
}

fn sidecar_is_fresh(csv_path: &Path, sidecar_path: &Path) -> bool {
    let Ok(csv_modified) = std::fs::metadata(csv_path).and_then(|m| m.modified()) else {
        return false;
    };
    let Ok(sidecar_modified) = std::fs::metadata(sidecar_path).and_then(|m| m.modified()) else {
        return false;
    };
    sidecar_modified >= csv_modified
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::bar::default_tick_size;
    use std::path::PathBuf;

    #[test]
    fn round_trip_pbar() {
        let csv =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/BTCUSDT_1d.csv");
        let tick = default_tick_size();
        let series = BarSeries::from_csv_path(&csv, tick).expect("csv");
        let dir = std::env::temp_dir().join("pallas_pbar_test");
        let _ = std::fs::create_dir_all(&dir);
        let pbar = dir.join("bars.pbar");
        write_pbar(&pbar, &series).expect("write");
        let loaded = read_pbar(&pbar).expect("read");
        assert_eq!(loaded.len(), series.len());
        assert_eq!(loaded.tick_size(), series.tick_size());
        assert_eq!(loaded.close_decimal(0), series.close_decimal(0));
    }

    #[test]
    fn csv_sidecar_pbar_is_reused_when_fresh() {
        let dir = std::env::temp_dir().join("pallas_bar_sidecar_test");
        std::fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("bars.csv");
        std::fs::write(
            &csv,
            "ts,open,high,low,close,volume\n2024-01-01,1,2,1,2,10\n",
        )
        .unwrap();
        let pbar = csv.with_extension("pbar");
        let _ = std::fs::remove_file(&pbar);

        let first = BarSeries::from_csv_path_or_pbar(&csv, default_tick_size()).unwrap();
        assert_eq!(first.len(), 1);
        assert!(pbar.is_file());

        let second = BarSeries::from_csv_path_or_pbar(&csv, default_tick_size()).unwrap();
        assert_eq!(second.len(), 1);
    }
}
