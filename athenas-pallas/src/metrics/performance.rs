//! Equity-curve and fill-ledger performance statistics.

use super::ledger::trade_ledger_from_fills;
use crate::events::FillRecord;
use crate::types::EquityPoint;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

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
    /// Fraction of realized closing fills with positive PnL (0 if none).
    pub win_rate: f64,
    /// Gross profit / gross loss; `f64::INFINITY` when no losing trades.
    pub profit_factor: f64,
    /// Count of fills that closed all or part of an open position.
    pub closed_trades: usize,
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
    fn pnl_matches() {
        let s = summarize(curve(), 252.0);
        assert_eq!(s.pnl, Decimal::from(20));
    }

    #[test]
    fn streaming_scalars_match_materialized_summary() {
        let equity = curve();
        let mut rolling = RollingMetrics::new();
        for point in &equity {
            rolling.record(point.equity_quote, 252.0);
        }

        let streaming = rolling.streaming_summary(252.0, &[], 0.05);
        let materialized = summarize_with_fills_and_rf(equity, 252.0, &[], 0.05);

        assert_eq!(streaming.pnl, materialized.pnl);
        assert_eq!(streaming.pnl_pct, materialized.pnl_pct);
        assert_eq!(streaming.closed_trades, materialized.closed_trades);
        for (actual, expected) in [
            (streaming.max_drawdown, materialized.max_drawdown),
            (streaming.sharpe, materialized.sharpe),
            (streaming.sortino, materialized.sortino),
            (streaming.win_rate, materialized.win_rate),
            (streaming.profit_factor, materialized.profit_factor),
        ] {
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        }
    }
}
