//! Equity-curve and fill-ledger performance statistics.

use super::positions::{strategy_position_report, StrategyPositionRow};
use crate::events::FillRecord;
use crate::state::GlobalState;
use crate::types::{EquityPoint, Side, StrategyId};
use rust_decimal::prelude::{Signed, ToPrimitive};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Summary statistics after a run.
#[derive(Clone, Debug)]
pub struct PerformanceSummary {
    /// Final minus initial equity.
    pub pnl: Decimal,
    /// PnL / initial equity.
    pub pnl_pct: Decimal,
    /// Max drawdown as positive fraction (0..1).
    pub max_drawdown: f64,
    /// Annualized-ish Sharpe using per-step returns (252 bars ~ 1y if bars are daily).
    pub sharpe: f64,
    /// Sortino using downside deviation.
    pub sortino: f64,
    /// Per-point simple returns (length n-1).
    pub returns: Vec<f64>,
    /// Copy of equity series.
    pub equity: Vec<EquityPoint>,
    /// Fraction of closed round-trips with positive PnL (0 if none).
    pub win_rate: f64,
    /// Gross profit / gross loss; `f64::INFINITY` when no losing trades.
    pub profit_factor: f64,
    /// Closed round-trip count from fill ledger.
    pub closed_trades: usize,
}

/// Round-trip trade statistics derived from an ordered fill blotter.
#[derive(Clone, Debug, Default)]
pub struct TradeLedger {
    /// Closed round trips.
    pub closed_trades: usize,
    /// Winning trades.
    pub wins: usize,
    /// Losing trades.
    pub losses: usize,
    /// `wins / closed_trades`.
    pub win_rate: f64,
    /// Sum of wins / sum of losses (absolute).
    pub profit_factor: f64,
    /// Total winning PnL.
    pub gross_profit: Decimal,
    /// Total losing PnL (positive magnitude).
    pub gross_loss: Decimal,
}

/// O(1) rolling drawdown and Welford return stats during replay.
///
/// Tracks enough state (first/last equity, full-sample and downside Welford accumulators, and
/// running max drawdown) to produce a complete [`PerformanceSummary`] via
/// [`RollingMetrics::streaming_summary`] without ever materializing a `Vec<EquityPoint>`. This is
/// what makes `record_equity_curve = false` runs use O(1) memory while keeping full metrics.
#[derive(Clone, Debug, Default)]
pub struct RollingMetrics {
    peak: Decimal,
    max_drawdown: f64,
    prev: Option<Decimal>,
    first: Option<Decimal>,
    last: Decimal,
    n_returns: usize,
    mean: f64,
    m2: f64,
    dn_count: usize,
    dn_mean: f64,
    dn_m2: f64,
}

impl RollingMetrics {
    /// New tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one equity sample.
    pub fn record(&mut self, equity: Decimal, _periods_per_year: f64) {
        let e = equity.to_f64().unwrap_or(0.0);
        if self.first.is_none() {
            self.first = Some(equity);
        }
        self.last = equity;
        if equity > self.peak {
            self.peak = equity;
        }
        let peak_f = self.peak.to_f64().unwrap_or(e);
        if peak_f > 0.0 {
            let dd = (peak_f - e) / peak_f;
            if dd > self.max_drawdown {
                self.max_drawdown = dd;
            }
        }
        if let Some(prev) = self.prev {
            let p = prev.to_f64().unwrap_or(1.0);
            if p.abs() > 1e-12 {
                let r = (e - p) / p;
                self.n_returns += 1;
                let delta = r - self.mean;
                self.mean += delta / self.n_returns as f64;
                let delta2 = r - self.mean;
                self.m2 += delta * delta2;
                if r < 0.0 {
                    self.dn_count += 1;
                    let d = r - self.dn_mean;
                    self.dn_mean += d / self.dn_count as f64;
                    let d2 = r - self.dn_mean;
                    self.dn_m2 += d * d2;
                }
            }
        }
        self.prev = Some(equity);
    }

    /// Peak-to-trough drawdown seen so far (0..1).
    pub fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }

    /// Annualized Sharpe from Welford stats.
    pub fn sharpe(&self, periods_per_year: f64) -> f64 {
        self.sharpe_excess(periods_per_year, 0.0)
    }

    fn sharpe_excess(&self, periods_per_year: f64, rf_per_period: f64) -> f64 {
        if self.n_returns < 2 {
            return 0.0;
        }
        let var = self.m2 / (self.n_returns - 1) as f64;
        let s = var.sqrt();
        if s < 1e-12 {
            return 0.0;
        }
        (self.mean - rf_per_period) / s * periods_per_year.sqrt()
    }

    fn sortino_excess(&self, periods_per_year: f64, rf_per_period: f64) -> f64 {
        if self.dn_count < 2 {
            return 0.0;
        }
        let ds = (self.dn_m2 / (self.dn_count - 1) as f64).sqrt();
        if ds < 1e-12 {
            return 0.0;
        }
        (self.mean - rf_per_period) / ds * periods_per_year.sqrt()
    }

    /// Build a full [`PerformanceSummary`] from streamed stats without an equity curve.
    ///
    /// `returns` and `equity` are left empty; all scalar metrics match the curve-based
    /// [`summarize_with_fills_and_rf`] within floating-point tolerance.
    pub fn streaming_summary(
        &self,
        periods_per_year: f64,
        fills: &[FillRecord],
        risk_free_annual: f64,
    ) -> PerformanceSummary {
        let ledger = trade_ledger_from_fills(fills);
        let first = self.first.unwrap_or(Decimal::ZERO);
        let pnl = self.last - first;
        let pnl_pct = if first.is_zero() {
            Decimal::ZERO
        } else {
            pnl / first
        };
        let rf_per_period = if periods_per_year > 0.0 {
            risk_free_annual / periods_per_year
        } else {
            0.0
        };
        PerformanceSummary {
            pnl,
            pnl_pct,
            max_drawdown: self.max_drawdown,
            sharpe: self.sharpe_excess(periods_per_year, rf_per_period),
            sortino: self.sortino_excess(periods_per_year, rf_per_period),
            returns: Vec::new(),
            equity: Vec::new(),
            win_rate: ledger.win_rate,
            profit_factor: ledger.profit_factor,
            closed_trades: ledger.closed_trades,
        }
    }
}

/// Compute summary. `periods_per_year` scales Sharpe/Sortino (e.g. 252 for daily bars).
pub fn summarize(equity: Vec<EquityPoint>, periods_per_year: f64) -> PerformanceSummary {
    summarize_with_fills_and_rf(equity, periods_per_year, &[], 0.0)
}

/// Like [`summarize`] but attaches trade ledger stats from fills.
pub fn summarize_with_fills(
    equity: Vec<EquityPoint>,
    periods_per_year: f64,
    fills: &[FillRecord],
) -> PerformanceSummary {
    summarize_with_fills_and_rf(equity, periods_per_year, fills, 0.0)
}

/// Like [`summarize_with_fills`] but subtracts `risk_free_annual` from Sharpe/Sortino.
pub fn summarize_with_fills_and_rf(
    equity: Vec<EquityPoint>,
    periods_per_year: f64,
    fills: &[FillRecord],
    risk_free_annual: f64,
) -> PerformanceSummary {
    let ledger = trade_ledger_from_fills(fills);
    if equity.is_empty() {
        return PerformanceSummary {
            pnl: Decimal::ZERO,
            pnl_pct: Decimal::ZERO,
            max_drawdown: 0.0,
            sharpe: 0.0,
            sortino: 0.0,
            returns: vec![],
            equity,
            win_rate: ledger.win_rate,
            profit_factor: ledger.profit_factor,
            closed_trades: ledger.closed_trades,
        };
    }
    let pnl = equity[equity.len() - 1].equity_quote - equity[0].equity_quote;
    let pnl_pct = if equity[0].equity_quote.is_zero() {
        Decimal::ZERO
    } else {
        pnl / equity[0].equity_quote
    };

    let rets: Vec<f64> = equity
        .windows(2)
        .map(|w| {
            let a = w[0].equity_quote.to_f64().unwrap_or(1.0);
            let b = w[1].equity_quote.to_f64().unwrap_or(1.0);
            if a.abs() < 1e-12 {
                0.0
            } else {
                (b - a) / a
            }
        })
        .collect();

    let max_dd = max_drawdown(&equity);
    let rf_per_period = if periods_per_year > 0.0 {
        risk_free_annual / periods_per_year
    } else {
        0.0
    };
    let sharpe = sharpe_ratio_excess(&rets, periods_per_year, rf_per_period);
    let sortino = sortino_ratio_excess(&rets, periods_per_year, rf_per_period);

    PerformanceSummary {
        pnl,
        pnl_pct,
        max_drawdown: max_dd,
        sharpe,
        sortino,
        returns: rets,
        equity,
        win_rate: ledger.win_rate,
        profit_factor: ledger.profit_factor,
        closed_trades: ledger.closed_trades,
    }
}

/// Realized round-trip PnL attributed to one sub-strategy, for the backtest report.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StrategyPnlRow {
    /// Sub-strategy label (from `OrderIntent::strategy_id`).
    pub strategy_id: String,
    /// Closed round-trip count for this strategy.
    pub closed_trades: usize,
    /// Fraction of closed round-trips with positive PnL.
    pub win_rate: f64,
    /// Gross profit / gross loss for this strategy.
    pub profit_factor: f64,
    /// Realized PnL (gross profit minus gross loss), as a decimal string.
    pub realized_pnl: String,
}

/// Per-strategy realized PnL from tagged fills (FIFO ledger run per [`StrategyId`]).
///
/// Untagged fills (no `strategy_id`) are excluded. Rows are sorted by `strategy_id` for
/// deterministic output. Returns empty when no fills carry an attribution.
pub fn per_strategy_pnl(fills: &[FillRecord]) -> Vec<StrategyPnlRow> {
    use std::collections::BTreeMap;
    let mut by_strategy: BTreeMap<String, Vec<FillRecord>> = BTreeMap::new();
    for fill in fills {
        if let Some(sid) = &fill.strategy_id {
            by_strategy
                .entry(sid.to_string())
                .or_default()
                .push(fill.clone());
        }
    }
    by_strategy
        .into_iter()
        .map(|(strategy_id, group)| {
            let ledger = trade_ledger_from_fills(&group);
            let realized = ledger.gross_profit - ledger.gross_loss;
            StrategyPnlRow {
                strategy_id,
                closed_trades: ledger.closed_trades,
                win_rate: ledger.win_rate,
                profit_factor: ledger.profit_factor,
                realized_pnl: realized.to_string(),
            }
        })
        .collect()
}

/// Build round-trip stats from chronological fills (FIFO position tracking).
pub fn trade_ledger_from_fills(fills: &[FillRecord]) -> TradeLedger {
    let mut position = Decimal::ZERO;
    let mut entry_price = Decimal::ZERO;
    let mut gross_profit = Decimal::ZERO;
    let mut gross_loss = Decimal::ZERO;
    let mut wins = 0usize;
    let mut losses = 0usize;
    let mut closed = 0usize;

    for fill in fills {
        let Ok(qty) = fill.qty.parse::<Decimal>() else {
            continue;
        };
        let Ok(price) = fill.price.parse::<Decimal>() else {
            continue;
        };
        let Ok(fee) = fill.fee.parse::<Decimal>() else {
            continue;
        };
        if qty.is_zero() {
            continue;
        }
        let sign = match fill.side {
            Side::Buy => Decimal::ONE,
            Side::Sell => -Decimal::ONE,
        };
        let delta = sign * qty;

        if position.is_zero() {
            position = delta;
            entry_price = price;
            continue;
        }

        if position.signum() == delta.signum() {
            let new_abs = position.abs() + qty;
            entry_price = (entry_price * position.abs() + price * qty) / new_abs;
            position += delta;
            continue;
        }

        let close_qty = qty.min(position.abs());
        let pnl_per_unit = (price - entry_price) * position.signum();
        let trade_pnl = pnl_per_unit * close_qty - fee;
        closed += 1;
        if trade_pnl > Decimal::ZERO {
            wins += 1;
            gross_profit += trade_pnl;
        } else if trade_pnl < Decimal::ZERO {
            losses += 1;
            gross_loss += trade_pnl.abs();
        }
        position += delta;
        if !position.is_zero() {
            entry_price = price;
        } else {
            entry_price = Decimal::ZERO;
        }
    }

    let win_rate = if closed == 0 {
        0.0
    } else {
        wins as f64 / closed as f64
    };
    let profit_factor = if gross_loss.is_zero() {
        if gross_profit.is_zero() {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        gross_profit.to_f64().unwrap_or(0.0) / gross_loss.to_f64().unwrap_or(1.0)
    };

    TradeLedger {
        closed_trades: closed,
        wins,
        losses,
        win_rate,
        profit_factor,
        gross_profit,
        gross_loss,
    }
}

/// Human-readable performance report (period label + risk-free reference + [`PerformanceSummary`]).
#[derive(Clone, Debug)]
pub struct TradingSummary {
    /// e.g. `"Daily"` or `"BacktestRun / momentum"` when built via [`trading_summaries_per_strategy`].
    pub period_label: String,
    /// Annualized risk-free rate used as reporting context (e.g. `0.05` for 5%).
    pub risk_free_annual: f64,
    /// Numeric performance bundle.
    pub performance: PerformanceSummary,
    /// Optional attributed open-base snapshot (fill with [`Self::with_strategy_attribution`]).
    pub strategy_attribution: Vec<StrategyPositionRow>,
}

impl TradingSummary {
    /// Build from an equity curve and scaling factor (same as [`summarize`]).
    pub fn from_equity(
        period_label: impl Into<String>,
        risk_free_annual: f64,
        equity: Vec<EquityPoint>,
        periods_per_year: f64,
    ) -> Self {
        Self {
            period_label: period_label.into(),
            risk_free_annual,
            performance: summarize(equity, periods_per_year),
            strategy_attribution: Vec::new(),
        }
    }

    /// Attach [`strategy_position_report`] from the final engine state (e.g. after a run).
    pub fn with_strategy_attribution(mut self, state: &GlobalState) -> Self {
        self.strategy_attribution = strategy_position_report(state);
        self
    }

    /// Print a one-line summary to stdout (for examples and quick CLI checks).
    pub fn print_summary(&self) {
        println!(
            "[{}] risk_free={:.4} pnl={} pnl_pct={} sharpe={:.3} sortino={:.3} max_dd={:.4}",
            self.period_label,
            self.risk_free_annual,
            self.performance.pnl,
            self.performance.pnl_pct,
            self.performance.sharpe,
            self.performance.sortino,
            self.performance.max_drawdown,
        );
        for row in &self.strategy_attribution {
            println!(
                "  strategy={} {} net_base={}",
                row.strategy_id, row.instrument, row.net_base_qty
            );
        }
    }
}

/// Build a [`TradingSummary`] per sub-strategy from **caller-supplied** equity curves (one series per [`StrategyId`]).
///
/// Each summary's [`TradingSummary::period_label`] is `{period_prefix} / {strategy_id}` so logs and exports stay distinct.
///
/// The engine attributes **positions** per strategy when fills carry [`StrategyId`]; account-level mark-to-market
/// equity is unchanged unless you record or model per-strategy curves yourself.
pub fn trading_summaries_per_strategy(
    period_prefix: impl Into<String>,
    risk_free_annual: f64,
    curves: HashMap<StrategyId, Vec<EquityPoint>>,
    periods_per_year: f64,
) -> HashMap<StrategyId, TradingSummary> {
    let period_prefix = period_prefix.into();
    curves
        .into_iter()
        .map(|(id, eq)| {
            let label = format!("{period_prefix} / {id}");
            (
                id,
                TradingSummary::from_equity(label, risk_free_annual, eq, periods_per_year),
            )
        })
        .collect()
}

fn max_drawdown(equity: &[EquityPoint]) -> f64 {
    let mut peak = f64::MIN;
    let mut max_dd = 0.0f64;
    for pt in equity {
        let v = pt.equity_quote.to_f64().unwrap_or(0.0);
        peak = peak.max(v);
        if peak > 0.0 {
            let dd = (peak - v) / peak;
            max_dd = max_dd.max(dd);
        }
    }
    max_dd
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn std_dev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    let v: f64 = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0);
    v.sqrt()
}

fn downside_std(xs: &[f64]) -> f64 {
    let mut count = 0usize;
    let mut mean = 0.0;
    let mut m2 = 0.0;
    for x in xs.iter().copied().filter(|r| *r < 0.0) {
        count += 1;
        let delta = x - mean;
        mean += delta / count as f64;
        let delta2 = x - mean;
        m2 += delta * delta2;
    }
    if count < 2 {
        return 0.0;
    }
    (m2 / (count as f64 - 1.0)).sqrt()
}

fn sharpe_ratio_excess(rets: &[f64], periods_per_year: f64, rf_per_period: f64) -> f64 {
    let m = mean(rets) - rf_per_period;
    let s = std_dev(rets);
    if s < 1e-12 {
        return 0.0;
    }
    (m / s) * periods_per_year.sqrt()
}

fn sortino_ratio_excess(rets: &[f64], periods_per_year: f64, rf_per_period: f64) -> f64 {
    let m = mean(rets) - rf_per_period;
    let ds = downside_std(rets);
    if ds < 1e-12 {
        return 0.0;
    }
    (m / ds) * periods_per_year.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn curve() -> Vec<EquityPoint> {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        vec![
            EquityPoint {
                ts: t0,
                equity_quote: Decimal::from(100),
            },
            EquityPoint {
                ts: t0,
                equity_quote: Decimal::from(110),
            },
            EquityPoint {
                ts: t0,
                equity_quote: Decimal::from(105),
            },
            EquityPoint {
                ts: t0,
                equity_quote: Decimal::from(120),
            },
        ]
    }

    #[test]
    fn mdd_nonzero() {
        let s = summarize(curve(), 252.0);
        assert!(s.max_drawdown > 0.0);
    }

    #[test]
    fn per_strategy_summaries_from_maps() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let mut m = HashMap::new();
        m.insert(
            StrategyId::new("a"),
            vec![
                EquityPoint {
                    ts: t0,
                    equity_quote: Decimal::from(100),
                },
                EquityPoint {
                    ts: t0,
                    equity_quote: Decimal::from(110),
                },
            ],
        );
        m.insert(
            StrategyId::new("b"),
            vec![
                EquityPoint {
                    ts: t0,
                    equity_quote: Decimal::from(50),
                },
                EquityPoint {
                    ts: t0,
                    equity_quote: Decimal::from(40),
                },
            ],
        );
        let out = trading_summaries_per_strategy("Daily", 0.0, m, 252.0);
        assert_eq!(out.len(), 2);
        let sa = out.get(&StrategyId::new("a")).unwrap();
        let sb = out.get(&StrategyId::new("b")).unwrap();
        assert_eq!(sa.performance.pnl, Decimal::from(10));
        assert_eq!(sb.performance.pnl, Decimal::from(-10));
        assert!(sa.period_label.contains("a"));
        assert!(sb.period_label.contains("b"));
    }

    #[test]
    fn pnl_matches() {
        let s = summarize(curve(), 252.0);
        assert_eq!(s.pnl, Decimal::from(20));
    }
}
