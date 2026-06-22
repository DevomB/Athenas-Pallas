//! Backtest run report types and serialization.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::events::FillRecord;
use crate::metrics::PerformanceSummary;
use crate::types::EquityPoint;

/// JSON-serializable run output.
#[derive(Clone, Debug, Serialize)]
pub struct BacktestReport {
    /// Net PnL in quote currency.
    pub pnl: String,
    /// PnL as fraction of starting equity.
    pub pnl_pct: String,
    /// Peak-to-trough drawdown (0..1).
    pub max_drawdown: f64,
    /// Annualized Sharpe ratio.
    pub sharpe: f64,
    /// Annualized Sortino ratio.
    pub sortino: f64,
    /// Number of fills.
    pub fill_count: u64,
    /// Mark-to-market equity samples.
    pub equity_curve: Vec<EquityPoint>,
    /// Per-fill blotter when recorded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<FillRecord>,
    /// Wall-clock runtime in milliseconds.
    pub wall_time_ms: u64,
    /// Fraction of closed round-trips with positive PnL.
    pub win_rate: f64,
    /// Gross profit / gross loss.
    pub profit_factor: f64,
    /// Closed round-trip count from fill ledger.
    pub closed_trades: usize,
    /// Per-sub-strategy realized PnL when fills carry a `strategy_id` (empty otherwise).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub per_strategy: Vec<crate::metrics::StrategyPnlRow>,
}

pub(crate) fn report_from_summary(
    s: PerformanceSummary,
    fill_count: u64,
    wall_time_ms: u64,
    fills: Vec<FillRecord>,
) -> BacktestReport {
    let per_strategy = crate::metrics::per_strategy_pnl(&fills);
    BacktestReport {
        pnl: s.pnl.to_string(),
        pnl_pct: s.pnl_pct.to_string(),
        max_drawdown: s.max_drawdown,
        sharpe: s.sharpe,
        sortino: s.sortino,
        fill_count,
        equity_curve: s.equity,
        fills,
        wall_time_ms,
        win_rate: s.win_rate,
        profit_factor: s.profit_factor,
        closed_trades: s.closed_trades,
        per_strategy,
    }
}

impl BacktestReport {
    /// Write pretty JSON to disk.
    pub fn write_json(&self, path: &Path) -> crate::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let mut f = File::create(path)?;
        f.write_all(json.as_bytes())?;
        Ok(())
    }
}
