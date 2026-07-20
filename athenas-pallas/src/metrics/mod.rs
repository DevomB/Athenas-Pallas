//! Performance metrics from an equity curve.

mod performance;
mod positions;

pub use performance::{
    per_strategy_pnl, summarize, summarize_with_fills, summarize_with_fills_and_rf,
    trade_ledger_from_fills, trading_summaries_per_strategy, PerformanceSummary, RollingMetrics,
    StrategyPnlRow, TradeLedger, TradingSummary,
};
pub use positions::{strategy_position_report, StrategyPositionRow};

#[cfg(test)]
mod tests {
    use super::positions::strategy_position_report;
    use crate::events::AccountEvent;
    use crate::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
    use crate::types::{Asset, InstrumentId, OrderId, Side, StrategyId};
    use rust_decimal::Decimal;
    use std::collections::HashMap;

    #[test]
    fn strategy_position_report_rows() {
        let i = InstrumentId::new("test", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
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
            client_order_id: None,
            oco_group: None,
            strategy_id: Some(StrategyId::new("z")),
        });
        let rows = strategy_position_report(&s);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].instrument, i);
        assert_eq!(rows[0].strategy_id, StrategyId::new("z"));
        assert_eq!(rows[0].net_base_qty, Decimal::ONE);
    }
}
