//! Equity-curve and fill-ledger performance statistics.

use crate::events::FillRecord;
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
    /// Fraction of realized closing fills with positive PnL (0 if none).
    pub win_rate: f64,
    /// Gross profit / gross loss; `f64::INFINITY` when no losing trades.
    pub profit_factor: f64,
    /// Count of fills that closed all or part of an open position.
    pub closed_trades: usize,
}

/// Realized trade statistics derived from an ordered fill blotter.
#[derive(Clone, Debug, Default)]
pub struct TradeLedger {
    /// Fills that closed all or part of an open position.
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

/// Realized PnL attributed to one sub-strategy, for the backtest report.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StrategyPnlRow {
    /// Sub-strategy label (from `OrderIntent::strategy_id`).
    pub strategy_id: String,
    /// Count of position-closing fills for this strategy.
    pub closed_trades: usize,
    /// Fraction of position-closing fills with positive PnL.
    pub win_rate: f64,
    /// Gross profit / gross loss for this strategy.
    pub profit_factor: f64,
    /// Realized PnL (gross profit minus gross loss), as a decimal string.
    pub realized_pnl: String,
}

/// Per-strategy realized PnL from tagged fills (average-cost ledger per instrument).
///
/// Untagged fills (no `strategy_id`) are excluded. Rows are sorted by `strategy_id` for
/// deterministic output. Returns empty when no fills carry an attribution.
pub fn per_strategy_pnl(fills: &[FillRecord]) -> Vec<StrategyPnlRow> {
    use std::collections::BTreeMap;
    let mut by_strategy: BTreeMap<&StrategyId, Vec<&FillRecord>> = BTreeMap::new();
    for fill in fills {
        if let Some(sid) = &fill.strategy_id {
            by_strategy.entry(sid).or_default().push(fill);
        }
    }
    by_strategy
        .into_iter()
        .map(|(strategy_id, group)| {
            let ledger = trade_ledger_from_fill_iter(group);
            let realized = ledger.gross_profit - ledger.gross_loss;
            StrategyPnlRow {
                strategy_id: strategy_id.to_string(),
                closed_trades: ledger.closed_trades,
                win_rate: ledger.win_rate,
                profit_factor: ledger.profit_factor,
                realized_pnl: realized.to_string(),
            }
        })
        .collect()
}

/// Build realized trade stats from chronological fills, tracking each instrument independently.
pub fn trade_ledger_from_fills(fills: &[FillRecord]) -> TradeLedger {
    trade_ledger_from_fill_iter(fills.iter())
}

fn trade_ledger_from_fill_iter<'a>(fills: impl IntoIterator<Item = &'a FillRecord>) -> TradeLedger {
    let mut positions = HashMap::new();
    let mut ledger = TradeLedger::default();

    for fill in fills {
        let Some((qty, price, fee, multiplier)) = parsed_fill(fill) else {
            continue;
        };
        let position = positions
            .entry(&fill.instrument)
            .or_insert_with(OpenPosition::default);
        if let Some(pnl) = position.apply(fill.side, qty, price, fee, multiplier) {
            ledger.record_close(pnl);
        }
    }

    ledger.finish()
}

fn parsed_fill(fill: &FillRecord) -> Option<(Decimal, Decimal, Decimal, Decimal)> {
    let qty = fill.qty.parse::<Decimal>().ok()?;
    let price = fill.price.parse::<Decimal>().ok()?;
    let fee = fill.fee.parse::<Decimal>().ok()?;
    let multiplier = fill
        .contract_multiplier
        .as_deref()
        .unwrap_or("1")
        .parse::<Decimal>()
        .ok()?;
    (qty > Decimal::ZERO && multiplier > Decimal::ZERO).then_some((qty, price, fee, multiplier))
}

#[derive(Default)]
struct OpenPosition {
    qty: Decimal,
    average_price: Decimal,
    entry_fees: Decimal,
}

impl OpenPosition {
    fn apply(
        &mut self,
        side: Side,
        qty: Decimal,
        price: Decimal,
        fee: Decimal,
        multiplier: Decimal,
    ) -> Option<Decimal> {
        let delta = match side {
            Side::Buy => qty,
            Side::Sell => -qty,
        };

        if self.qty.is_zero() {
            self.qty = delta;
            self.average_price = price;
            self.entry_fees = fee;
            return None;
        }

        if self.qty.signum() == delta.signum() {
            let new_abs = self.qty.abs() + qty;
            self.average_price = (self.average_price * self.qty.abs() + price * qty) / new_abs;
            self.qty += delta;
            self.entry_fees += fee;
            return None;
        }

        let old_abs = self.qty.abs();
        let close_qty = qty.min(old_abs);
        let opening_fee = self.entry_fees * close_qty / old_abs;
        let closing_fee = fee * close_qty / qty;
        let pnl = (price - self.average_price) * self.qty.signum() * close_qty * multiplier
            - opening_fee
            - closing_fee;

        self.qty += delta;
        if self.qty.is_zero() {
            self.average_price = Decimal::ZERO;
            self.entry_fees = Decimal::ZERO;
        } else if self.qty.signum() == delta.signum() {
            self.average_price = price;
            self.entry_fees = fee - closing_fee;
        } else {
            self.entry_fees -= opening_fee;
        }
        Some(pnl)
    }
}

impl TradeLedger {
    fn record_close(&mut self, pnl: Decimal) {
        self.closed_trades += 1;
        if pnl > Decimal::ZERO {
            self.wins += 1;
            self.gross_profit += pnl;
        } else if pnl < Decimal::ZERO {
            self.losses += 1;
            self.gross_loss += pnl.abs();
        }
    }

    fn finish(mut self) -> Self {
        self.win_rate = if self.closed_trades == 0 {
            0.0
        } else {
            self.wins as f64 / self.closed_trades as f64
        };
        self.profit_factor = if self.gross_loss.is_zero() {
            if self.gross_profit.is_zero() {
                0.0
            } else {
                f64::INFINITY
            }
        } else {
            self.gross_profit.to_f64().unwrap_or(0.0) / self.gross_loss.to_f64().unwrap_or(1.0)
        };
        self
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
    use crate::types::{InstrumentId, OrderId};
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

    fn fill(symbol: &str, side: Side, qty: &str, price: &str, fee: &str) -> FillRecord {
        FillRecord {
            ts: OffsetDateTime::UNIX_EPOCH,
            order_id: OrderId::new_v4(),
            instrument: InstrumentId::new("test", symbol),
            side,
            qty: qty.into(),
            price: price.into(),
            fee: fee.into(),
            contract_multiplier: None,
            client_order_id: None,
            oco_group: None,
            strategy_id: None,
        }
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
    fn trade_ledger_charges_opening_and_closing_fees() {
        let ledger = trade_ledger_from_fills(&[
            fill("ABC", Side::Buy, "1", "100", "1"),
            fill("ABC", Side::Sell, "1", "102", "1"),
        ]);

        assert_eq!(ledger.closed_trades, 1);
        assert_eq!(ledger.win_rate, 0.0);
        assert_eq!(ledger.gross_profit, Decimal::ZERO);
        assert_eq!(ledger.gross_loss, Decimal::ZERO);
    }

    #[test]
    fn trade_ledger_applies_contract_multiplier_once() {
        let mut opening = fill("ES", Side::Buy, "1", "100", "1");
        let mut closing = fill("ES", Side::Sell, "1", "102", "1");
        opening.contract_multiplier = Some("50".into());
        closing.contract_multiplier = Some("50".into());

        let ledger = trade_ledger_from_fills(&[opening, closing]);

        assert_eq!(ledger.closed_trades, 1);
        assert_eq!(ledger.gross_profit, Decimal::from(98));
        assert_eq!(ledger.gross_loss, Decimal::ZERO);
    }

    #[test]
    fn trade_ledger_keeps_instruments_independent() {
        let ledger = trade_ledger_from_fills(&[
            fill("ABC", Side::Buy, "1", "100", "0"),
            fill("XYZ", Side::Buy, "1", "1000", "0"),
            fill("ABC", Side::Sell, "1", "110", "0"),
            fill("XYZ", Side::Sell, "1", "900", "0"),
        ]);

        assert_eq!(ledger.closed_trades, 2);
        assert_eq!(ledger.wins, 1);
        assert_eq!(ledger.losses, 1);
        assert_eq!(ledger.gross_profit, Decimal::from(10));
        assert_eq!(ledger.gross_loss, Decimal::from(100));
    }

    #[test]
    fn trade_ledger_allocates_fees_across_partial_close_and_reversal() {
        let ledger = trade_ledger_from_fills(&[
            fill("ABC", Side::Buy, "2", "100", "2"),
            fill("ABC", Side::Sell, "1", "110", "1"),
            fill("ABC", Side::Sell, "2", "90", "2"),
            fill("ABC", Side::Buy, "1", "80", "1"),
        ]);

        assert_eq!(ledger.closed_trades, 3);
        assert_eq!(ledger.wins, 2);
        assert_eq!(ledger.losses, 1);
        assert_eq!(ledger.gross_profit, Decimal::from(16));
        assert_eq!(ledger.gross_loss, Decimal::from(12));
    }
}
