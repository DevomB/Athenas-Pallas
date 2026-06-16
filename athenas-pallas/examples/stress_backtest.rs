//! Large CSV backtest smoke test (memory + throughput).

use athenas_pallas::backtest::{run_backtest, BacktestConfig, DataFormat};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bars: u64 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let dir = env::temp_dir().join("pallas_stress");
    std::fs::create_dir_all(&dir)?;
    let csv = dir.join(format!("stress_{bars}.csv"));
    write_synthetic_csv(&csv, bars)?;

    let mut balances = HashMap::new();
    balances.insert(
        athenas_pallas::types::Asset("USDT".into()),
        Decimal::from(1_000_000u64),
    );

    let cfg = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Ohlcv,
        instrument: athenas_pallas::types::InstrumentId::new("binance", "BTCUSDT"),
        balances,
        record_equity_curve: false,
        ..BacktestConfig::default()
    };

    let started = Instant::now();
    let report = run_backtest(&cfg)?;
    let elapsed = started.elapsed();
    println!(
        "bars={bars} fills={} pnl={} elapsed_ms={} rss_hint=run_with/usr/bin/time -v on Linux",
        report.fill_count,
        report.pnl,
        elapsed.as_millis()
    );
    Ok(())
}

fn write_synthetic_csv(path: &PathBuf, n: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut w = BufWriter::new(File::create(path)?);
    writeln!(w, "ts,open,high,low,close,volume")?;
    let mut px = 40_000u64;
    for i in 0..n {
        let day = i / 1440 + 1;
        let hour = (i % 1440) / 60;
        let min = i % 60;
        writeln!(
            w,
            "2024-01-{day:02}T{hour:02}:{min:02}:00Z,{px},{},{},{},1",
            px + 10,
            px - 10,
            px + 1
        )?;
        px = px.wrapping_add(1);
    }
    Ok(())
}
