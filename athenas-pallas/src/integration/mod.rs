//! Low-level transport helpers shared by venue connectors.
//!
//! WebSocket clients typically use [`ws_connect_async`] (re-export of `tokio_tungstenite::connect_async`).

#[cfg(feature = "binance")]
pub use tokio_tungstenite::connect_async as ws_connect_async;
