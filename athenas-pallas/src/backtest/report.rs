//! Backtest run report types and serialization.
#![allow(missing_docs)]

use std::fs::File;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::events::{FillRecord, RejectionRecord};
use crate::metrics::PerformanceSummary;
use crate::types::{
    ClientOrderId, EquityPoint, InstrumentId, OrderId, OrderType, Side, StrategyId,
};

/// Effective run settings serialized with every report.
#[derive(Clone, Debug, Default, Serialize)]
pub struct BacktestParameters {
    pub fee_bps: String,
    pub slippage_bps: String,
    pub half_spread_bps: String,
    pub buy_and_hold_qty: Option<String>,
    pub periods_per_year: f64,
    pub bar_interval: Option<String>,
    pub session_filter: Option<String>,
    pub risk_free_annual: f64,
    pub max_position_abs: Option<String>,
    pub max_daily_loss_quote: Option<String>,
    pub margin_initial_rate: Option<String>,
    pub record_equity_curve: bool,
    pub strategy_path: Option<String>,
    pub strategy_parameters: std::collections::HashMap<String, serde_json::Value>,
    pub initial_balances: std::collections::BTreeMap<String, String>,
}

/// One configured historical source.
#[derive(Clone, Debug, Serialize)]
pub struct DataSourceMetadata {
    pub instrument: InstrumentId,
    pub path: Option<String>,
    pub format: String,
}

/// Replay input metadata and observed time range.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DataMetadata {
    pub sources: Vec<DataSourceMetadata>,
    pub processed_events: u64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub start: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end: Option<time::OffsetDateTime>,
}

/// Final net position for one registered instrument.
#[derive(Clone, Debug, Serialize)]
pub struct FinalPosition {
    pub instrument: InstrumentId,
    pub qty: String,
}

/// Working venue order or accepted bar order awaiting the next market update.
#[derive(Clone, Debug, Serialize)]
pub struct PendingOrder {
    pub order_id: Option<OrderId>,
    pub instrument: InstrumentId,
    pub side: Side,
    pub order_type: OrderType,
    pub qty: String,
    pub price: Option<String>,
    pub stop_price: Option<String>,
    pub client_order_id: Option<ClientOrderId>,
    pub oco_group: Option<String>,
    pub strategy_id: Option<StrategyId>,
    pub state: String,
}

pub(crate) struct ReportDetails {
    pub parameters: BacktestParameters,
    pub data: DataMetadata,
    pub fills: Vec<FillRecord>,
    pub total_fees: String,
    pub turnover: String,
    pub risk_rejection_count: u64,
    pub execution_rejection_count: u64,
    pub rejections: Vec<RejectionRecord>,
    pub pending_orders: Vec<PendingOrder>,
    pub final_positions: Vec<FinalPosition>,
}

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
    /// Effective run settings, including arbitrary external-strategy parameters.
    pub parameters: BacktestParameters,
    /// Input source and observed replay-range metadata.
    pub data: DataMetadata,
    /// Sum of all fill fees.
    pub total_fees: String,
    /// Gross traded notional across all fills.
    pub turnover: String,
    /// Number of risk-rule rejections.
    pub risk_rejection_count: u64,
    /// Number of execution-layer rejections.
    pub execution_rejection_count: u64,
    /// Structured rejection details.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rejections: Vec<RejectionRecord>,
    /// Orders still working or awaiting a future market update at replay end.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pending_orders: Vec<PendingOrder>,
    /// Final net positions for every registered instrument.
    pub final_positions: Vec<FinalPosition>,
}

pub(crate) fn report_from_summary(
    s: PerformanceSummary,
    wall_time_ms: u64,
    details: ReportDetails,
) -> BacktestReport {
    let per_strategy = crate::metrics::per_strategy_pnl(&details.fills);
    let fill_count = details.fills.len() as u64;
    BacktestReport {
        pnl: s.pnl.to_string(),
        pnl_pct: s.pnl_pct.to_string(),
        max_drawdown: s.max_drawdown,
        sharpe: s.sharpe,
        sortino: s.sortino,
        fill_count,
        equity_curve: s.equity,
        fills: details.fills,
        wall_time_ms,
        win_rate: s.win_rate,
        profit_factor: s.profit_factor,
        closed_trades: s.closed_trades,
        per_strategy,
        parameters: details.parameters,
        data: details.data,
        total_fees: details.total_fees,
        turnover: details.turnover,
        risk_rejection_count: details.risk_rejection_count,
        execution_rejection_count: details.execution_rejection_count,
        rejections: details.rejections,
        pending_orders: details.pending_orders,
        final_positions: details.final_positions,
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
