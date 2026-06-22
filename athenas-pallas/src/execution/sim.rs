//! Simulation gateway - delegates to [`super::fills::FillEngine`] fill logic.

use super::{AccountEvents, FillEngine, PaperConfig, SyncExecutionGateway};
use crate::error::Result;
use crate::events::OrderIntent;
use crate::state::GlobalState;
use crate::types::OrderId;

/// Backtest / replay gateway (same crossing rules as paper).
#[derive(Clone)]
pub struct SimGateway {
    inner: FillEngine,
}

impl SimGateway {
    /// New simulator with fee and slippage knobs.
    pub fn new(cfg: PaperConfig) -> Self {
        Self {
            inner: FillEngine::new(cfg),
        }
    }
}

impl Default for SimGateway {
    fn default() -> Self {
        Self::new(PaperConfig::default())
    }
}

impl SyncExecutionGateway for SimGateway {
    fn place_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<AccountEvents> {
        self.inner.place_limit_sync(state, intent)
    }

    fn place_market(&self, state: &GlobalState, intent: &OrderIntent) -> Result<AccountEvents> {
        self.inner.place_market_sync(state, intent)
    }

    fn place_stop_market(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<AccountEvents> {
        self.inner.place_stop_market_sync(state, intent)
    }

    fn place_stop_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<AccountEvents> {
        self.inner.place_stop_limit_sync(state, intent)
    }

    fn cancel(&self, state: &GlobalState, order_id: OrderId) -> Result<AccountEvents> {
        self.inner.cancel_sync(state, order_id)
    }

    fn cancel_all(&self, state: &GlobalState) -> Result<AccountEvents> {
        self.inner.cancel_all_sync(state)
    }

    fn poll_after_market(&self, state: &GlobalState) -> Result<AccountEvents> {
        self.inner.poll_after_market_sync(state)
    }

    fn poll_after_market_instrument(
        &self,
        state: &GlobalState,
        instrument: &crate::types::InstrumentId,
    ) -> Result<AccountEvents> {
        self.inner
            .poll_after_market_instrument_sync(state, instrument)
    }
}
