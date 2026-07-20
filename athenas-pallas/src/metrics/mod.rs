//! Performance metrics from an equity curve and fill ledger.

mod ledger;
mod performance;
mod positions;

pub use ledger::{per_strategy_pnl, trade_ledger_from_fills, StrategyPnlRow, TradeLedger};
pub use performance::{
    summarize, summarize_with_fills, summarize_with_fills_and_rf, PerformanceSummary,
    RollingMetrics,
};
pub use positions::{strategy_position_report, StrategyPositionRow};

#[cfg(test)]
mod tests {
    use super::strategy_position_report;
    use crate::events::AccountEvent;
    use crate::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
    use crate::types::{Asset, InstrumentId, OrderId, Side, StrategyId};
    use rust_decimal::Decimal;
    use std::collections::HashMap;

    #[test]
    fn strategy_position_report_rows() {
        let instrument = InstrumentId::new("test", "BTCUSDT");
        let instruments = HashMap::from([(
            instrument.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        )]);
        let mut state = GlobalState::new(
            InstrumentRegistry::from_instruments(instruments),
            HashMap::new(),
        );
        state.apply_account(&AccountEvent::Fill {
            order_id: OrderId::new_v4(),
            instrument: instrument.clone(),
            side: Side::Buy,
            price: Decimal::ONE,
            qty: Decimal::ONE,
            fee: Decimal::ZERO,
            fee_asset: Asset("USDT".into()),
            client_order_id: None,
            oco_group: None,
            strategy_id: Some(StrategyId::new("z")),
        });

        let rows = strategy_position_report(&state);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].instrument, instrument);
        assert_eq!(rows[0].strategy_id, StrategyId::new("z"));
        assert_eq!(rows[0].net_base_qty, Decimal::ONE);
    }
}
