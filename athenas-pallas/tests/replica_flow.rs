//! Audit replica tracks pause / trading-disable flags.

use athenas_pallas::audit::{ControlEventSummary, EngineAudit, StrategySkipReason};
use athenas_pallas::replica::EngineStateReplica;
use athenas_pallas::types::TradingState;

#[test]
fn replica_applies_control_and_skip_audits() {
    let mut replica = EngineStateReplica::new();
    replica.apply(&EngineAudit::ControlApplied {
        control: ControlEventSummary::Pause,
    });
    assert!(replica.paused);
    replica.apply(&EngineAudit::StrategySkipped {
        reason: StrategySkipReason::TradingDisabled,
    });
    // Skip events are counted; trading state changes only via control events.
    assert_eq!(replica.trading_state, TradingState::Enabled);
    assert_eq!(replica.strategy_skips, 1);
    replica.apply(&EngineAudit::ControlApplied {
        control: ControlEventSummary::DisableTrading,
    });
    assert_eq!(replica.trading_state, TradingState::Disabled);
}
