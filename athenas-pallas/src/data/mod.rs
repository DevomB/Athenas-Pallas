//! Market data subscription kinds.
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

#[cfg(feature = "databento")]
pub mod databento;
