//! Market data subscription kinds and stream-oriented docs (barter-data style).
//!
//! Connectors implement [`crate::connectors::MarketConnector`] and emit normalized
//! [`crate::events::MarketEvent`] values into an [`crate::engine::EngineHandle`].
//!
//! ## Multi-venue fan-in
//!
//! Run **one async task per venue or stream**, each holding a clone of the same
//! [`crate::engine::EngineHandle`]. All tasks `send` into the same MPSC queue so the engine
//! remains a single consumer. Normalize venue payloads to [`crate::events::MarketEvent`] inside
//! each connector; keep the strategy and state venue-agnostic.

/// What public data a connector subscribes to (for documentation and future mux builders).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubKind {
    /// Public trades.
    PublicTrade,
    /// Top-of-book best bid/ask.
    BookL1,
    /// Bounded depth snapshot.
    BookL2Snapshot,
}
