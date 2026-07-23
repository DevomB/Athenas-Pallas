//! Realized PnL statistics from chronological fill records.

use crate::events::FillRecord;
use crate::types::{Side, StrategyId};
use rust_decimal::prelude::{Signed, ToPrimitive};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};

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
/// Untagged fills are excluded. Rows are sorted by strategy id for deterministic output.
pub fn per_strategy_pnl(fills: &[FillRecord]) -> Vec<StrategyPnlRow> {
    let mut by_strategy: BTreeMap<&StrategyId, Vec<&FillRecord>> = BTreeMap::new();
    for fill in fills {
        if let Some(strategy_id) = &fill.strategy_id {
            by_strategy.entry(strategy_id).or_default().push(fill);
        }
    }
    by_strategy
        .into_iter()
        .map(|(strategy_id, fills)| {
            let ledger = trade_ledger_from_fill_iter(fills);
            StrategyPnlRow {
                strategy_id: strategy_id.to_string(),
                closed_trades: ledger.closed_trades,
                win_rate: ledger.win_rate,
                profit_factor: ledger.profit_factor,
                realized_pnl: (ledger.gross_profit - ledger.gross_loss).to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InstrumentId, OrderId};
    use time::OffsetDateTime;

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
            simulation_model: None,
            client_order_id: None,
            oco_group: None,
            strategy_id: None,
        }
    }

    #[test]
    fn charges_opening_and_closing_fees() {
        let ledger = trade_ledger_from_fills(&[
            fill("ABC", Side::Buy, "1", "100", "1"),
            fill("ABC", Side::Sell, "1", "102", "1"),
        ]);
        assert_eq!(ledger.closed_trades, 1);
        assert_eq!(ledger.gross_profit, Decimal::ZERO);
        assert_eq!(ledger.gross_loss, Decimal::ZERO);
    }

    #[test]
    fn applies_contract_multiplier_once() {
        let mut opening = fill("ES", Side::Buy, "1", "100", "1");
        let mut closing = fill("ES", Side::Sell, "1", "102", "1");
        opening.contract_multiplier = Some("50".into());
        closing.contract_multiplier = Some("50".into());

        let ledger = trade_ledger_from_fills(&[opening, closing]);
        assert_eq!(ledger.gross_profit, Decimal::from(98));
        assert_eq!(ledger.gross_loss, Decimal::ZERO);
    }

    #[test]
    fn keeps_instruments_independent() {
        let ledger = trade_ledger_from_fills(&[
            fill("ABC", Side::Buy, "1", "100", "0"),
            fill("XYZ", Side::Buy, "1", "1000", "0"),
            fill("ABC", Side::Sell, "1", "110", "0"),
            fill("XYZ", Side::Sell, "1", "900", "0"),
        ]);
        assert_eq!(
            (ledger.closed_trades, ledger.wins, ledger.losses),
            (2, 1, 1)
        );
        assert_eq!(ledger.gross_profit, Decimal::from(10));
        assert_eq!(ledger.gross_loss, Decimal::from(100));
    }

    #[test]
    fn allocates_fees_across_partial_close_and_reversal() {
        let ledger = trade_ledger_from_fills(&[
            fill("ABC", Side::Buy, "2", "100", "2"),
            fill("ABC", Side::Sell, "1", "110", "1"),
            fill("ABC", Side::Sell, "2", "90", "2"),
            fill("ABC", Side::Buy, "1", "80", "1"),
        ]);
        assert_eq!(
            (ledger.closed_trades, ledger.wins, ledger.losses),
            (3, 2, 1)
        );
        assert_eq!(ledger.gross_profit, Decimal::from(16));
        assert_eq!(ledger.gross_loss, Decimal::from(12));
    }
}
