//! Backtest result persistence (JSON and JSONL ledger).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::backtest::BacktestReport;
use crate::error::Result;

/// Write a pretty-printed JSON backtest report.
pub fn write_backtest_json(path: &Path, report: &BacktestReport) -> Result<()> {
    report.write_json(path)
}

/// Append one JSON line to a results ledger (`results.jsonl`).
pub fn append_results_jsonl(path: &Path, report: &BacktestReport) -> Result<()> {
    let line = serde_json::to_string(report)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(crate::Error::Io)?;
    writeln!(f, "{line}").map_err(crate::Error::Io)?;
    Ok(())
}

/// Write JSON report and optionally append to a ledger when `ledger_path` is set.
pub fn write_backtest_outputs(
    json_path: &Path,
    report: &BacktestReport,
    ledger_path: Option<&Path>,
) -> Result<()> {
    write_backtest_json(json_path, report)?;
    if let Some(ledger) = ledger_path {
        append_results_jsonl(ledger, report)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EquityPoint;
    use rust_decimal::Decimal;
    use time::macros::datetime;

    #[test]
    fn jsonl_append_two_lines() {
        let dir = std::env::temp_dir().join("pallas_results_test");
        let _ = std::fs::create_dir_all(&dir);
        let ledger = dir.join("results.jsonl");
        let _ = std::fs::remove_file(&ledger);
        let report = BacktestReport {
            pnl: "1".into(),
            pnl_pct: "0.01".into(),
            max_drawdown: 0.1,
            sharpe: 1.0,
            sortino: 1.0,
            fill_count: 1,
            equity_curve: vec![EquityPoint {
                ts: datetime!(2024-01-01 00:00:00 UTC),
                equity_quote: Decimal::from(100u64),
            }],
            fills: vec![],
            wall_time_ms: 1,
            win_rate: 0.5,
            profit_factor: 2.0,
            closed_trades: 1,
            per_strategy: vec![],
            parameters: crate::backtest::BacktestParameters::default(),
            data: crate::backtest::DataMetadata::default(),
            total_fees: "0".into(),
            turnover: "0".into(),
            risk_rejection_count: 0,
            execution_rejection_count: 0,
            rejections: vec![],
            pending_orders: vec![],
            final_positions: vec![],
        };
        append_results_jsonl(&ledger, &report).unwrap();
        append_results_jsonl(&ledger, &report).unwrap();
        let text = std::fs::read_to_string(&ledger).unwrap();
        assert_eq!(text.lines().count(), 2);
        let first: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(
            first["equity_curve"][0]["ts"],
            serde_json::Value::String("2024-01-01T00:00:00Z".into())
        );
    }
}
