//! Engine audit stream for monitoring, replicas, and persistence (barter-style).

use crate::events::{ControlEvent, Event, OrderIntent};

/// High-level audit tick emitted on a [`tokio::sync::broadcast`] channel when configured.
#[derive(Clone, Debug)]
pub enum EngineAudit {
    /// Ingested input event (summary only; full payloads stay on the hot path via `Event`).
    EventIngested {
        /// Event time when known (Unix nanoseconds UTC).
        ts_unix_ns: Option<i128>,
        /// Short discriminator.
        kind: EngineAuditEventKind,
    },
    /// Strategy evaluation skipped (algo off or operator pause).
    StrategySkipped {
        /// Reason.
        reason: StrategySkipReason,
    },
    /// Risk pipeline rejected an intent.
    RiskRejected {
        /// Copy of the intent that failed.
        intent: OrderIntent,
        /// Human-readable reason.
        message: String,
    },
    /// Execution returned an error placing or canceling.
    ExecutionError {
        /// Error message.
        message: String,
    },
    /// One or more account events applied after successful execution.
    IntentsExecuted {
        /// Number of account events merged into state.
        account_events: usize,
    },
    /// Engine loop ended.
    Shutdown(ShutdownAudit),
    /// Control plane action applied (before strategy gate).
    ControlApplied {
        /// Which control event.
        control: ControlEventSummary,
    },
}

/// Final audit marker returned from [`crate::system::System::shutdown`].
#[derive(Clone, Debug, Default)]
pub struct ShutdownAudit;

/// Summary of [`crate::events::ControlEvent`].
#[derive(Clone, Debug)]
pub enum ControlEventSummary {
    /// Operator pause.
    Pause,
    /// Operator resume.
    Resume,
    /// Cancel all working orders.
    CancelAll,
    /// Flatten positions.
    Flatten,
    /// Disable algorithmic trading.
    DisableTrading,
    /// Re-enable algorithmic trading.
    EnableTrading,
}

impl From<&ControlEvent> for ControlEventSummary {
    fn from(c: &ControlEvent) -> Self {
        match c {
            ControlEvent::Pause => ControlEventSummary::Pause,
            ControlEvent::Resume => ControlEventSummary::Resume,
            ControlEvent::CancelAll => ControlEventSummary::CancelAll,
            ControlEvent::Flatten => ControlEventSummary::Flatten,
            ControlEvent::DisableTrading => ControlEventSummary::DisableTrading,
            ControlEvent::EnableTrading => ControlEventSummary::EnableTrading,
        }
    }
}

/// Coarse event kind for audits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineAuditEventKind {
    /// Public market data.
    Market,
    /// Account or execution feedback.
    Account,
    /// Control-plane event.
    Control,
    /// Timer tick.
    Timer,
}

/// Why the strategy hook was not run for this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategySkipReason {
    /// Operator pause (`GlobalState::paused`).
    Paused,
    /// Algorithmic trading off (`GlobalState::trading_state` disabled).
    TradingDisabled,
}

impl From<&Event> for EngineAuditEventKind {
    fn from(ev: &Event) -> Self {
        match ev {
            Event::Market(_) => EngineAuditEventKind::Market,
            Event::Account(_) => EngineAuditEventKind::Account,
            Event::Control(_) => EngineAuditEventKind::Control,
            Event::Timer(_) => EngineAuditEventKind::Timer,
        }
    }
}

fn event_ts_unix_ns(ev: &Event) -> Option<i128> {
    ev.timestamp_unix_nanos()
}

/// Best-effort send: lagging subscribers are dropped by `broadcast` (no engine blocking).
pub(crate) fn try_emit(tx: &tokio::sync::broadcast::Sender<EngineAudit>, msg: EngineAudit) {
    let _ = tx.send(msg);
}

pub(crate) fn emit_event_ingested(tx: &tokio::sync::broadcast::Sender<EngineAudit>, ev: &Event) {
    try_emit(
        tx,
        EngineAudit::EventIngested {
            ts_unix_ns: event_ts_unix_ns(ev),
            kind: ev.into(),
        },
    );
}
