//! Performance metrics from an equity curve.

use crate::instrument::InstrumentIndex;
use crate::state::GlobalState;
use crate::types::{EquityPoint, InstrumentId, StrategyId};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// One non-zero **attributed** net base position (see [`GlobalState::strategy_positions`](crate::state::GlobalState::strategy_positions)).
#[derive(Clone, Debug, PartialEq)]
pub struct StrategyPositionRow {
    /// Instrument.
    pub instrument: InstrumentId,
    /// Sub-strategy id from fills / orders.
    pub strategy_id: StrategyId,
    /// Signed net base quantity.
    pub net_base_qty: Decimal,
}

/// Collect attributed positions for reporting (table-style tear-sheet input).
///
/// Only entries with non-zero qty are returned, sorted by `strategy_id` then `instrument`.
pub fn strategy_position_report(state: &GlobalState) -> Vec<StrategyPositionRow> {
    let mut rows: Vec<StrategyPositionRow> = state
        .strategy_positions
        .iter()
        .filter(|(_, q)| !q.is_zero())
        .filter_map(|((ix, sid), qty)| {
            let inst = state.registry.id(InstrumentIndex(*ix))?.clone();
            Some(StrategyPositionRow {
                instrument: inst,
                strategy_id: sid.clone(),
                net_base_qty: *qty,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a.strategy_id
            .cmp(&b.strategy_id)
            .then_with(|| a.instrument.cmp(&b.instrument))
    });
    rows
}

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
}

/// Compute summary. `periods_per_year` scales Sharpe/Sortino (e.g. 252 for daily bars).
pub fn summarize(equity: Vec<EquityPoint>, periods_per_year: f64) -> PerformanceSummary {
    if equity.is_empty() {
        return PerformanceSummary {
            pnl: Decimal::ZERO,
            pnl_pct: Decimal::ZERO,
            max_drawdown: 0.0,
            sharpe: 0.0,
            sortino: 0.0,
            returns: vec![],
            equity,
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
    let sharpe = sharpe_ratio(&rets, periods_per_year);
    let sortino = sortino_ratio(&rets, periods_per_year);

    PerformanceSummary {
        pnl,
        pnl_pct,
        max_drawdown: max_dd,
        sharpe,
        sortino,
        returns: rets,
        equity,
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
    let neg: Vec<f64> = xs.iter().copied().filter(|r| *r < 0.0).collect();
    if neg.len() < 2 {
        return 0.0;
    }
    let m = mean(&neg);
    let v: f64 = neg.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (neg.len() as f64 - 1.0);
    v.sqrt()
}

fn sharpe_ratio(rets: &[f64], periods_per_year: f64) -> f64 {
    let m = mean(rets);
    let s = std_dev(rets);
    if s < 1e-12 {
        return 0.0;
    }
    (m / s) * periods_per_year.sqrt()
}

fn sortino_ratio(rets: &[f64], periods_per_year: f64) -> f64 {
    let m = mean(rets);
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
    fn strategy_position_report_rows() {
        use crate::events::AccountEvent;
        use crate::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
        use crate::types::{Asset, InstrumentId, OrderId, Side};

        let i = InstrumentId::new("t", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            InstrumentMeta {
                base: Asset("BTC".into()),
                quote: Asset("USDT".into()),
            },
        );
        let mut s = GlobalState::new(InstrumentRegistry::from_instruments(inst), HashMap::new());
        s.apply_account(&AccountEvent::Fill {
            order_id: OrderId::new_v4(),
            instrument: i.clone(),
            side: Side::Buy,
            price: Decimal::ONE,
            qty: Decimal::ONE,
            fee: Decimal::ZERO,
            fee_asset: Asset("USDT".into()),
            strategy_id: Some(StrategyId::new("z")),
        });
        let rows = super::strategy_position_report(&s);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].instrument, i);
        assert_eq!(rows[0].strategy_id, StrategyId::new("z"));
        assert_eq!(rows[0].net_base_qty, Decimal::ONE);
    }

    #[test]
    fn pnl_matches() {
        let s = summarize(curve(), 252.0);
        assert_eq!(s.pnl, Decimal::from(20));
    }
}
