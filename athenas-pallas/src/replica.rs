//! Engine state replica from audit stream (barter parity).

use crate::audit::EngineAudit;
use crate::state::GlobalState;
use crate::types::TradingState;
use tokio::sync::broadcast;

/// Read-only mirror updated from [`EngineAudit`] ticks.
#[derive(Clone, Debug, Default)]
pub struct EngineStateReplica {
    /// Mirrored flags.
    pub paused: bool,
    /// Mirrored trading state.
    pub trading_state: TradingState,
    /// Count of ingested market events.
    pub market_events: u64,
    /// Risk rejects observed.
    pub risk_rejects: u64,
    /// Strategy skips observed.
    pub strategy_skips: u64,
}

impl EngineStateReplica {
    /// New empty replica.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one audit message.
    pub fn apply(&mut self, audit: &EngineAudit) {
        match audit {
            EngineAudit::EventIngested { kind, .. } => {
                use crate::audit::EngineAuditEventKind;
                if *kind == EngineAuditEventKind::Market {
                    self.market_events += 1;
                }
            }
            EngineAudit::StrategySkipped { .. } => {
                self.strategy_skips += 1;
            }
            EngineAudit::RiskRejected { .. } => self.risk_rejects += 1,
            EngineAudit::ControlApplied { control } => {
                use crate::audit::ControlEventSummary;
                match control {
                    ControlEventSummary::Pause => self.paused = true,
                    ControlEventSummary::Resume => self.paused = false,
                    ControlEventSummary::DisableTrading => {
                        self.trading_state = TradingState::Disabled
                    }
                    ControlEventSummary::EnableTrading => {
                        self.trading_state = TradingState::Enabled
                    }
                    _ => {}
                }
            }
            EngineAudit::Shutdown(_) => {}
            _ => {}
        }
    }

    /// Consume broadcast receiver until lagged or closed.
    pub async fn run(mut self, mut rx: broadcast::Receiver<EngineAudit>) {
        loop {
            match rx.recv().await {
                Ok(a) => {
                    if matches!(a, EngineAudit::Shutdown(_)) {
                        break;
                    }
                    self.apply(&a);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

/// Compare replica flags to authoritative state (integration tests).
pub fn replica_matches_state(replica: &EngineStateReplica, state: &GlobalState) -> bool {
    replica.paused == state.paused && replica.trading_state == state.trading_state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{ControlEventSummary, StrategySkipReason};

    #[test]
    fn control_applied_pause_resume_updates_replica() {
        let mut replica = EngineStateReplica::new();
        replica.apply(&EngineAudit::ControlApplied {
            control: ControlEventSummary::Pause,
        });
        assert!(replica.paused);
        replica.apply(&EngineAudit::ControlApplied {
            control: ControlEventSummary::Resume,
        });
        assert!(!replica.paused);
    }

    #[test]
    fn strategy_skipped_does_not_change_trading_state() {
        let mut replica = EngineStateReplica::new();
        replica.apply(&EngineAudit::StrategySkipped {
            reason: StrategySkipReason::TradingDisabled,
        });
        assert_eq!(replica.trading_state, TradingState::Enabled);
        assert_eq!(replica.strategy_skips, 1);
    }
}
