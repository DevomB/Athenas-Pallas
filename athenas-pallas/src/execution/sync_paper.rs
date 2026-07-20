//! Synchronous execution for backtest hot loops.

use crate::error::Result;
use crate::events::OrderIntent;
use crate::state::GlobalState;
use crate::types::OrderId;

use super::{AccountEvents, FillEngine};

/// Sync venue bridge for deterministic replay without async overhead.
pub trait SyncExecutionGateway: Send + Sync {
    /// Resting or crossing limit.
    fn place_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<AccountEvents>;
    /// Immediate market.
    fn place_market(&self, state: &GlobalState, intent: &OrderIntent) -> Result<AccountEvents>;
    /// Stop market.
    fn place_stop_market(
        &self,
        state: &GlobalState,
        intent: &OrderIntent,
    ) -> Result<AccountEvents> {
        let _ = (state, intent);
        Err(crate::error::Error::Invalid(
            "stop market not supported by this gateway".into(),
        ))
    }
    /// Stop limit.
    fn place_stop_limit(&self, state: &GlobalState, intent: &OrderIntent) -> Result<AccountEvents> {
        let _ = (state, intent);
        Err(crate::error::Error::Invalid(
            "stop limit not supported by this gateway".into(),
        ))
    }
    /// Cancel one order.
    fn cancel(&self, state: &GlobalState, order_id: OrderId) -> Result<AccountEvents>;
    /// Cancel all working orders.
    fn cancel_all(&self, state: &GlobalState) -> Result<AccountEvents>;
    /// Passive fills after a market event.
    fn poll_after_market(&self, state: &GlobalState) -> Result<AccountEvents> {
        let _ = state;
        Ok(AccountEvents::new())
    }
    /// Passive fills after a market event on a known instrument.
    ///
    /// Defaults to the full-book [`Self::poll_after_market`]; gateways with an instrument index
    /// (e.g. paper/sim) override this to only evaluate orders on the instrument that ticked.
    fn poll_after_market_instrument(
        &self,
        state: &GlobalState,
        instrument: &crate::types::InstrumentId,
    ) -> Result<AccountEvents> {
        let _ = instrument;
        self.poll_after_market(state)
    }
}

/// Backtest gateway delegating to [`FillEngine`] sync fill rules.
#[derive(Clone)]
pub struct PaperExecution {
    inner: FillEngine,
}

impl PaperExecution {
    /// New paper execution backend.
    pub fn new(cfg: super::PaperConfig) -> Self {
        Self {
            inner: FillEngine::new(cfg),
        }
    }
}

impl Default for PaperExecution {
    fn default() -> Self {
        Self::new(super::PaperConfig::default())
    }
}

impl SyncExecutionGateway for PaperExecution {
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
