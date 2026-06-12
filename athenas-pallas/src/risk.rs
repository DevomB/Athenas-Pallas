//! Risk checks applied before execution.

use crate::error::Result;
use crate::events::{OrderIntent, OrderIntentSource};
use crate::state::GlobalState;
use crate::types::{Asset, TradingState};
use rust_decimal::Decimal;

/// Single risk rule.
pub trait RiskCheck: Send + Sync {
    /// Validate intent against current snapshot.
    fn check(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()>;
}

/// Max absolute position size (base units) per instrument.
#[derive(Clone, Debug)]
pub struct MaxPositionSize {
    /// Instrument.
    pub instrument: crate::types::InstrumentId,
    /// Max absolute base position after intent.
    pub max_abs: Decimal,
}

impl RiskCheck for MaxPositionSize {
    fn check(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        if intent.instrument != self.instrument {
            return Ok(());
        }
        let current = state.position_qty(&self.instrument);
        let sign = match intent.side {
            crate::types::Side::Buy => Decimal::ONE,
            crate::types::Side::Sell => -Decimal::ONE,
        };
        let projected = current + sign * intent.qty;
        if projected.abs() > self.max_abs {
            return Err(crate::error::Error::RiskRejected(format!(
                "position {projected} exceeds max {}",
                self.max_abs
            )));
        }
        Ok(())
    }
}

/// Block all new orders when paused flag is set.
#[derive(Clone, Debug, Default)]
pub struct PauseCheck;

impl RiskCheck for PauseCheck {
    fn check(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        if intent.source == OrderIntentSource::Flatten {
            return Ok(());
        }
        if state.paused {
            return Err(crate::error::Error::RiskRejected("engine paused".into()));
        }
        Ok(())
    }
}

/// Block new orders when [`crate::types::TradingState::Disabled`] (flatten still allowed).
#[derive(Clone, Debug, Default)]
pub struct TradingDisabledCheck;

impl RiskCheck for TradingDisabledCheck {
    fn check(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        if intent.source == OrderIntentSource::Flatten {
            return Ok(());
        }
        if state.trading_state == TradingState::Disabled {
            return Err(crate::error::Error::RiskRejected("trading disabled".into()));
        }
        Ok(())
    }
}

/// Block new orders when daily mark-to-market equity loss (UTC day, vs day-start anchor) exceeds limit.
#[derive(Clone, Debug)]
pub struct MaxDailyLossQuote {
    /// Equity denominator asset (must match [`crate::state::GlobalState::daily_risk_quote`]).
    pub quote: Asset,
    /// Maximum allowed drop from day-start equity in `quote` units.
    pub max_loss: Decimal,
}

impl RiskCheck for MaxDailyLossQuote {
    fn check(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        if intent.source == OrderIntentSource::Flatten {
            return Ok(());
        }
        let Some((_, day_start_eq)) = state.risk_day_anchor else {
            return Ok(());
        };
        let current = state.mark_equity_quote(&self.quote);
        let loss = day_start_eq - current;
        if loss > self.max_loss {
            return Err(crate::error::Error::RiskRejected(format!(
                "max daily loss exceeded: {loss} {} (limit {})",
                self.quote.0, self.max_loss
            )));
        }
        Ok(())
    }
}

/// Block short sales for equity instruments.
#[derive(Clone, Debug, Default)]
pub struct RejectShort;

impl RiskCheck for RejectShort {
    fn check(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        if intent.side != crate::types::Side::Sell {
            return Ok(());
        }
        let Some(meta) = state.registry.meta_by_id(&intent.instrument) else {
            return Ok(());
        };
        if meta.asset_class != crate::instrument::AssetClass::Equity {
            return Ok(());
        }
        let current = state.position_qty(&intent.instrument);
        if current - intent.qty < Decimal::ZERO {
            return Err(crate::error::Error::RiskRejected(
                "short selling not allowed for equity".into(),
            ));
        }
        Ok(())
    }
}

/// Holistic risk manager (barter-style); builds a [`RiskPipeline`].
pub trait RiskManager: Send + Sync {
    /// Compose checks for the engine.
    fn pipeline(&self) -> RiskPipeline;
}

/// Default checks: pause, trading disabled, optional daily loss.
#[derive(Clone, Debug, Default)]
pub struct DefaultRiskManager {
    /// Optional max daily loss in quote units.
    pub max_daily_loss: Option<MaxDailyLossQuote>,
}

impl RiskManager for DefaultRiskManager {
    fn pipeline(&self) -> RiskPipeline {
        let mut checks: Vec<Box<dyn RiskCheck + Send + Sync>> = vec![
            Box::new(PauseCheck),
            Box::new(TradingDisabledCheck),
        ];
        if let Some(rule) = &self.max_daily_loss {
            checks.push(Box::new(rule.clone()));
        }
        RiskPipeline::new(checks)
    }
}

/// Inline pause + reject-short for synchronous backtest replay.
#[derive(Clone, Copy, Debug, Default)]
pub struct BacktestChecks;

impl BacktestChecks {
    /// Run standard backtest risk rules without dynamic dispatch.
    #[inline]
    pub fn validate(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        PauseCheck.check(state, intent)?;
        RejectShort.check(state, intent)?;
        Ok(())
    }
}

/// Ordered pipeline of checks.
#[derive(Clone)]
pub struct RiskPipeline {
    checks: std::sync::Arc<Vec<Box<dyn RiskCheck + Send + Sync>>>,
}

impl RiskPipeline {
    /// New pipeline.
    pub fn new(checks: Vec<Box<dyn RiskCheck + Send + Sync>>) -> Self {
        Self {
            checks: std::sync::Arc::new(checks),
        }
    }

    /// Run all checks.
    pub fn validate(&self, state: &GlobalState, intent: &OrderIntent) -> Result<()> {
        for c in self.checks.iter() {
            c.check(state, intent)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::InstrumentRegistry;
    use crate::types::{Asset, InstrumentId, OrderType, Side};
    use std::collections::HashMap;

    #[test]
    fn max_position_rejects() {
        let i = InstrumentId::new("t", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            crate::state::InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(10_000u64));
        let st = GlobalState::new(InstrumentRegistry::from_instruments(inst), bal);
        let rule = MaxPositionSize {
            instrument: i.clone(),
            max_abs: Decimal::ONE,
        };
        let intent = OrderIntent {
            instrument: i,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            qty: Decimal::from(5u64),
            client_order_id: None,
            source: OrderIntentSource::User,
            strategy_id: None,
        };
        assert!(rule.check(&st, &intent).is_err());
    }

    #[test]
    fn pause_blocks_user_not_flatten() {
        let i = InstrumentId::new("t", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            crate::state::InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(10_000u64));
        let mut st = GlobalState::new(InstrumentRegistry::from_instruments(inst), bal);
        st.paused = true;
        let pause = PauseCheck::default();
        let mut intent = OrderIntent {
            instrument: i.clone(),
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            source: OrderIntentSource::User,
            strategy_id: None,
        };
        assert!(pause.check(&st, &intent).is_err());
        intent.source = OrderIntentSource::Flatten;
        assert!(pause.check(&st, &intent).is_ok());
    }

    #[test]
    fn trading_disabled_blocks_user_not_flatten() {
        let i = InstrumentId::new("t", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            crate::state::InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(10_000u64));
        let mut st = GlobalState::new(InstrumentRegistry::from_instruments(inst), bal);
        st.trading_state = crate::types::TradingState::Disabled;
        let gate = TradingDisabledCheck::default();
        let mut intent = OrderIntent {
            instrument: i.clone(),
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            source: OrderIntentSource::User,
            strategy_id: None,
        };
        assert!(gate.check(&st, &intent).is_err());
        intent.source = OrderIntentSource::Flatten;
        assert!(gate.check(&st, &intent).is_ok());
    }

    #[test]
    fn max_daily_loss_blocks_after_anchor_drop() {
        let i = InstrumentId::new("t", "BTCUSDT");
        let mut inst = HashMap::new();
        inst.insert(
            i.clone(),
            crate::state::InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut bal = HashMap::new();
        bal.insert(Asset("USDT".into()), Decimal::from(8500u64));
        let mut st = GlobalState::new(InstrumentRegistry::from_instruments(inst), bal);
        st.daily_risk_quote = Some(Asset("USDT".into()));
        st.risk_day_anchor = Some((time::OffsetDateTime::now_utc().date(), Decimal::from(10_000u64)));

        let rule = MaxDailyLossQuote {
            quote: Asset("USDT".into()),
            max_loss: Decimal::from(1000u64),
        };
        let intent = OrderIntent {
            instrument: i,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            source: OrderIntentSource::User,
            strategy_id: None,
        };
        assert!(rule.check(&st, &intent).is_err());

        let mut intent_f = intent.clone();
        intent_f.source = OrderIntentSource::Flatten;
        assert!(rule.check(&st, &intent_f).is_ok());
    }
}
