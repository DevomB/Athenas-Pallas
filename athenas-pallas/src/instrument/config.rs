//! System configuration (barter `system_config.json` shape).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Full system config file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Risk-free rate for Sharpe/Sortino.
    #[serde(default)]
    pub risk_free_return: Decimal,
    /// Tradable instruments.
    pub instruments: Vec<InstrumentConfig>,
    /// Mock/live execution backends.
    #[serde(default)]
    pub executions: Vec<ExecutionConfig>,
}

/// One instrument row in config.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstrumentConfig {
    /// Exchange id (e.g. `binance_spot`).
    pub exchange: String,
    /// Venue symbol (e.g. `BTCUSDT`).
    pub name_exchange: String,
    /// Base/quote underlying.
    pub underlying: UnderlyingConfig,
    /// Quote denomination hint.
    #[serde(default)]
    pub quote: Option<String>,
    /// `spot`, `perpetual`, `future`, `option`.
    pub kind: String,
    /// Future/option expiry.
    #[serde(default)]
    pub expiry: Option<String>,
    /// Option strike.
    #[serde(default)]
    pub strike: Option<String>,
    /// `call` or `put`.
    #[serde(default)]
    pub option_kind: Option<String>,
}

/// Underlying in config JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnderlyingConfig {
    /// Base asset.
    pub base: String,
    /// Quote asset.
    pub quote: String,
}

/// Mock execution backend config.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Which exchange to mock.
    pub mocked_exchange: String,
    /// Simulated latency ms.
    #[serde(default)]
    pub latency_ms: u64,
    /// Fee percent (e.g. 0.05 = 0.05%).
    #[serde(default)]
    pub fees_percent: Decimal,
    /// Initial balances and orders.
    pub initial_state: MockInitialState,
}

/// Initial mock exchange state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockInitialState {
    /// Exchange id.
    pub exchange: String,
    /// Balances.
    pub balances: Vec<BalanceSnapshot>,
    /// Per-instrument open orders.
    pub instruments: Vec<MockInstrumentState>,
}

/// Balance row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceSnapshot {
    /// Asset.
    pub asset: String,
    /// Total/free.
    pub balance: BalanceAmount,
    /// Exchange timestamp.
    #[serde(default)]
    pub time_exchange: Option<String>,
}

/// Total and free balance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceAmount {
    /// Total.
    pub total: Decimal,
    /// Free.
    pub free: Decimal,
}

/// Instrument slice in mock state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockInstrumentState {
    /// Symbol.
    pub instrument: String,
    /// Open orders (empty for fresh start).
    #[serde(default)]
    pub orders: Vec<serde_json::Value>,
}
