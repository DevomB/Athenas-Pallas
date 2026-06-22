//! Error types.

use thiserror::Error;

/// Top-level framework error.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O or transport failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse/serialize.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Decimal parse.
    #[error("decimal parse: {0}")]
    Decimal(#[from] rust_decimal::Error),
    /// Risk rejected order.
    #[error("risk rejected: {0}")]
    RiskRejected(String),
    /// Execution rejected.
    #[error("execution rejected: {0}")]
    ExecutionRejected(String),
    /// Invalid configuration or state.
    #[error("invalid: {0}")]
    Invalid(String),
    /// External strategy protocol failure.
    #[error("strategy protocol: {0}")]
    StrategyProtocol(String),
    /// Backtest cancelled via cooperative flag.
    #[error("backtest cancelled")]
    Cancelled,
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;
