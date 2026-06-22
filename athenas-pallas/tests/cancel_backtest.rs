use athenas_pallas::backtest::{run_backtest_with_cancel, BacktestConfig, DataFormat};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn cancel_flag_stops_large_run() {
    let tmp = std::env::temp_dir().join("pallas_cancel_100k.csv");
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "ts,open,high,low,close,volume").unwrap();
        for _ in 0..50_000 {
            writeln!(f, "2024-01-01 00:00:00,40000,40100,39900,40000,1").unwrap();
        }
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = cancel.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(5));
        cancel_worker.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    let cfg = BacktestConfig {
        data_path: PathBuf::from(&tmp),
        data_format: DataFormat::Ohlcv,
        record_equity_curve: false,
        ..BacktestConfig::default()
    };

    let started = std::time::Instant::now();
    let err = run_backtest_with_cancel(&cfg, Some(cancel)).unwrap_err();
    assert!(matches!(err, athenas_pallas::Error::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "cancel took {:?}",
        started.elapsed()
    );
}
