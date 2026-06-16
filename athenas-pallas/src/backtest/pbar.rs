//! Binary bar cache (`.pbar`) for fast replay without CSV parsing.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use rust_decimal::Decimal;

use super::bar::{Bar, BarSeries};

const MAGIC: &[u8; 4] = b"PBAR";
const VERSION: u32 = 1;

/// Write a [`BarSeries`] to a `.pbar` file.
pub fn write_pbar(path: &Path, series: &BarSeries) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    f.write_all(MAGIC)?;
    f.write_all(&VERSION.to_le_bytes())?;
    let tick = series.tick_size();
    let tick_str = tick.to_string();
    let tick_len = tick_str.len() as u32;
    f.write_all(&tick_len.to_le_bytes())?;
    f.write_all(tick_str.as_bytes())?;
    let count = series.len() as u64;
    f.write_all(&count.to_le_bytes())?;
    for i in 0..series.len() {
        let bar = series.get(i).expect("bar index");
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
    let mut bars = Vec::with_capacity(count);
    for _ in 0..count {
        let mut i8 = [0u8; 8];
        f.read_exact(&mut i8)?;
        let ts_unix_nanos = i64::from_le_bytes(i8);
        f.read_exact(&mut i8)?;
        let open_ticks = i64::from_le_bytes(i8);
        f.read_exact(&mut i8)?;
        let high_ticks = i64::from_le_bytes(i8);
        f.read_exact(&mut i8)?;
        let low_ticks = i64::from_le_bytes(i8);
        f.read_exact(&mut i8)?;
        let close_ticks = i64::from_le_bytes(i8);
        f.read_exact(&mut i8)?;
        let volume_lots = i64::from_le_bytes(i8);
        bars.push(Bar {
            ts_unix_nanos,
            open_ticks,
            high_ticks,
            low_ticks,
            close_ticks,
            volume_lots,
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

/// True when path looks like a `.pbar` cache file.
pub fn is_pbar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pbar"))
        .unwrap_or(false)
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
}
