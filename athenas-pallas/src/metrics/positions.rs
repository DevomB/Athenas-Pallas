//! Attributed position reporting from engine state.

use crate::instrument::InstrumentIndex;
use crate::state::GlobalState;
use crate::types::{InstrumentId, StrategyId};
use rust_decimal::Decimal;

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
