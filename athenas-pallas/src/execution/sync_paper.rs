//! Synchronous execution for backtest hot loops.

use crate::error::Result;
use crate::events::{AccountEvent, OrderIntent};
use crate::state::GlobalState;
use crate::types::OrderId;

use super::PaperGateway;

/// Sync venue bridge for deterministic replay without async overhead.
pub trait SyncExecutionGateway: Send + Sync {
    /// Resting or crossing limit.
    fn place_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>>;
    /// Immediate market.
    fn place_market(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>>;
    /// Stop market.
    fn place_stop_market(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        let _ = (state, intent);
        Err(crate::error::Error::Invalid(
            "stop market not supported by this gateway".into(),
        ))
    }
    /// Stop limit.
    fn place_stop_limit(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<Vec<AccountEvent>> {
        let _ = (state, intent);
        Err(crate::error::Error::Invalid(
            "stop limit not supported by this gateway".into(),
        ))
    }
    /// Cancel one order.
    fn cancel(&self, state: &GlobalState, order_id: OrderId) -> Result<Vec<AccountEvent>>;
    /// Cancel all working orders.
    fn cancel_all(&self, state: &GlobalState) -> Result<Vec<AccountEvent>>;
    /// Passive fills after a market event.
    fn poll_after_market(&self, state: &GlobalState) -> Result<Vec<AccountEvent>> {
        let _ = state;
        Ok(vec![])
    }
}

/// Backtest gateway delegating to [`PaperGateway`] sync fill rules.
#[derive(Clone)]
pub struct SyncPaperGateway {
    inner: PaperGateway,
}

impl SyncPaperGateway {
    /// New sync paper gateway.
    pub fn new(cfg: super::PaperConfig) -> Self {
        Self {
            inner: PaperGateway::new(cfg),
        }
    }
}

impl SyncExecutionGateway for SyncPaperGateway {
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
