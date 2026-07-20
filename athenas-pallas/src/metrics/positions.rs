//! Attributed position reporting from engine state.

use crate::instrument::InstrumentIndex;
use crate::state::GlobalState;
use crate::types::{InstrumentId, StrategyId};
use rust_decimal::Decimal;

/// One non-zero attributed net base position.
#[derive(Clone, Debug, PartialEq)]
pub struct StrategyPositionRow {
    /// Instrument.
    pub instrument: InstrumentId,
    /// Sub-strategy id from fills or orders.
    pub strategy_id: StrategyId,
    /// Signed net base quantity.
    pub net_base_qty: Decimal,
}

/// Collect non-zero attributed positions, sorted by strategy then instrument.
pub fn strategy_position_report(state: &GlobalState) -> Vec<StrategyPositionRow> {
    let mut rows: Vec<_> = state
        .strategy_positions
        .iter()
        .filter(|(_, qty)| !qty.is_zero())
        .filter_map(|((index, strategy_id), qty)| {
            Some(StrategyPositionRow {
                instrument: state.registry.id(InstrumentIndex(*index))?.clone(),
                strategy_id: strategy_id.clone(),
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
