//! Simulation gateway - delegates to [`super::paper::PaperGateway`] fill logic.

use async_trait::async_trait;

use super::{ExecutionGateway, PaperConfig, PaperGateway, SyncExecutionGateway};
use crate::error::Result;
use crate::events::{AccountEvent, OrderIntent};
use crate::state::GlobalState;
use crate::types::OrderId;

/// Backtest / replay gateway (same crossing rules as paper).
#[derive(Clone)]
pub struct SimGateway {
    inner: PaperGateway,
}

impl SimGateway {
    /// New simulator with fee and sl slippage knobs.
    pub fn new(cfg: PaperConfig) -> Self {
        Self {
            inner: PaperGateway::new(cfg),
        }
    }
}

impl Default for SimGateway {
    fn default() -> Self {
        Self::new(PaperConfig::default())
    }
}

#[async_trait]
impl ExecutionGateway for SimGateway {
    async fn place_limit(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        self.inner.place_limit_sync(state, intent)
    }

    async fn place_market(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        self.inner.place_market_sync(state, intent)
    }

    async fn place_stop_market(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        self.inner.place_stop_market_sync(state, intent)
    }

    async fn place_stop_limit(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        self.inner.place_stop_limit_sync(state, intent)
    }

    async fn cancel(&self, state: &GlobalState, order_id: OrderId) -> Result<Vec<AccountEvent>> {
        self.inner.cancel_sync(state, order_id)
    }

    async fn cancel_all(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        self.inner.cancel_all_sync(state)
    }

    async fn poll_after_market(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        self.inner.poll_after_market_sync(state)
    }
}

impl SyncExecutionGateway for SimGateway {
    fn place_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>> {
        self.inner.place_limit_sync(state, intent)
    }

    fn place_market(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>> {
        self.inner.place_market_sync(state, intent)
    }

    fn place_stop_market(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        self.inner.place_stop_market_sync(state, intent)
    }

    fn place_stop_limit(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        self.inner.place_stop_limit_sync(state, intent)
    }

    fn cancel(&self, state: &GlobalState, order_id: OrderId) -> Result<Vec<AccountEvent>> {
        self.inner.cancel_sync(state, order_id)
    }

    fn cancel_all(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        self.inner.cancel_all_sync(state)
    }

    fn poll_after_market(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        self.inner.poll_after_market_sync(state)
    }
}
