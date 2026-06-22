//! SystemBuilder orchestration (barter parity).

use crate::audit::{EngineAudit, ShutdownAudit};
use crate::engine::{
    dispatch_event_audited, EngineBuilder, EngineConfig, EngineHandle, TimerSchedule,
};
use crate::error::Result;
use crate::events::Event;
use crate::execution::ExecutionGateway;
use crate::instrument::{IndexedInstruments, InstrumentFilter, SystemConfig};
use crate::metrics::TradingSummary;
use crate::risk::{RiskManager, RiskPipeline};
use crate::state::GlobalState;
use crate::strategy::Strategy;
use crate::types::{EquityPoint, TradingState};
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;

/// Engine feed mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EngineFeedMode {
    /// Tokio MPSC async feed (live).
    #[default]
    Async,
    /// Synchronous iterator (backtest).
    Iterator,
}

/// Audit mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuditMode {
    /// No audit broadcast.
    #[default]
    Disabled,
    /// Enabled.
    Enabled,
}

/// Summary period label.
#[derive(Clone, Copy, Debug)]
pub enum SummaryPeriod {
    /// Daily bars.
    Daily,
    /// Custom label.
    Custom(&'static str),
}

impl SummaryPeriod {
    fn label(self) -> &'static str {
        match self {
            SummaryPeriod::Daily => "Daily",
            SummaryPeriod::Custom(s) => s,
        }
    }
}

/// Clock abstraction.
pub trait Clock: Send + Sync {
    /// Wall time now (for timers / risk).
    fn now(&self) -> time::OffsetDateTime;
}

/// Live wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveClock;

impl Clock for LiveClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

/// System construction arguments.
pub struct SystemArgs<'a, S, E> {
    /// Indexed instruments.
    pub instruments: &'a IndexedInstruments,
    /// Loaded config.
    pub config: SystemConfig,
    /// Clock.
    pub clock: Arc<dyn Clock>,
    /// Strategy.
    pub strategy: S,
    /// Risk manager.
    pub risk: Arc<dyn RiskManager>,
    /// Execution gateway.
    pub execution: Arc<E>,
    /// Initial global state (built from registry).
    pub state: GlobalState,
}

impl<'a, S, E> SystemArgs<'a, S, E> {
    /// New args.
    pub fn new(
        instruments: &'a IndexedInstruments,
        config: SystemConfig,
        clock: Arc<dyn Clock>,
        strategy: S,
        risk: Arc<dyn RiskManager>,
        execution: Arc<E>,
        state: GlobalState,
    ) -> Self {
        Self {
            instruments,
            config,
            clock,
            strategy,
            risk,
            execution,
            state,
        }
    }
}

/// System builder.
#[derive(Clone)]
pub struct SystemBuilder {
    feed_mode: EngineFeedMode,
    audit_mode: AuditMode,
    trading_state: TradingState,
    channel_capacity: usize,
    audit_capacity: usize,
    command_capacity: Option<usize>,
    timer_schedules: Vec<TimerSchedule>,
    equity_instrument: Option<crate::types::InstrumentId>,
}

impl Default for SystemBuilder {
    fn default() -> Self {
        Self {
            feed_mode: EngineFeedMode::Async,
            audit_mode: AuditMode::Disabled,
            trading_state: TradingState::Enabled,
            channel_capacity: 4096,
            audit_capacity: 1024,
            command_capacity: Some(64),
            timer_schedules: vec![],
            equity_instrument: None,
        }
    }
}

impl SystemBuilder {
    /// New builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Instrument used when sampling equity during [`System::run_iterator`].
    pub fn equity_instrument(mut self, inst: crate::types::InstrumentId) -> Self {
        self.equity_instrument = Some(inst);
        self
    }

    /// Feed mode.
    pub fn engine_feed_mode(mut self, mode: EngineFeedMode) -> Self {
        self.feed_mode = mode;
        self
    }

    /// Audit mode.
    pub fn audit_mode(mut self, mode: AuditMode) -> Self {
        self.audit_mode = mode;
        self
    }

    /// Initial trading state.
    pub fn trading_state(mut self, state: TradingState) -> Self {
        self.trading_state = state;
        self
    }

    /// Build system (does not spawn until [`System::init_with_runtime`]).
    pub fn build<S, E>(self, args: SystemArgs<'_, S, E>) -> Result<System<S, E>>
    where
        S: Strategy + Send + 'static,
        E: ExecutionGateway + 'static,
    {
        let mut state = args.state;
        state.trading_state = self.trading_state;
        let risk = args.risk.pipeline();
        let equity_instrument = self.equity_instrument.clone();
        Ok(System {
            handle: EngineHandle::disconnected(),
            join: None,
            audit_rx: None,
            state: Arc::new(Mutex::new(state)),
            strategy: Some(args.strategy),
            risk,
            exec: args.execution,
            config: args.config,
            trading_state: self.trading_state,
            equity_curve: vec![],
            equity_instrument,
            builder: self,
        })
    }
}

/// Built system (may be running).
pub struct System<S, E> {
    handle: EngineHandle,
    join: Option<JoinHandle<Result<()>>>,
    audit_rx: Option<broadcast::Receiver<EngineAudit>>,
    state: Arc<Mutex<GlobalState>>,
    strategy: Option<S>,
    risk: RiskPipeline,
    exec: Arc<E>,
    config: SystemConfig,
    trading_state: TradingState,
    equity_curve: Vec<EquityPoint>,
    equity_instrument: Option<crate::types::InstrumentId>,
    builder: SystemBuilder,
}

/// External handle for runtime control.
pub struct SystemHandle {
    inner: EngineHandle,
}

impl SystemHandle {
    /// Forward event to engine.
    pub async fn send(&self, ev: Event) -> Result<()> {
        self.inner.send(ev).await
    }

    /// Set trading state via control event.
    pub async fn trading_state(&self, state: TradingState) -> Result<()> {
        let ev = match state {
            TradingState::Enabled => Event::Control(crate::events::ControlEvent::EnableTrading),
            TradingState::Disabled => Event::Control(crate::events::ControlEvent::DisableTrading),
        };
        self.inner.send(ev).await
    }

    /// Cancel orders for filter.
    pub async fn cancel_orders(&self, filter: InstrumentFilter) -> Result<()> {
        match filter {
            InstrumentFilter::None | InstrumentFilter::All => {
                self.inner
                    .send(Event::Control(crate::events::ControlEvent::CancelAll))
                    .await
            }
            InstrumentFilter::One(legacy) => {
                let inst = crate::types::InstrumentId::new(legacy.exchange, legacy.symbol);
                let (tx, rx) = oneshot::channel();
                self.inner
                    .send_engine_command(crate::engine::EngineCommand::CancelOrdersInstrument {
                        instrument: inst,
                        reply: tx,
                    })
                    .await?;
                let _ = rx.await;
                Ok(())
            }
        }
    }

    /// Close positions for filter.
    pub async fn close_positions(&self, filter: InstrumentFilter) -> Result<()> {
        match filter {
            InstrumentFilter::None | InstrumentFilter::All => {
                self.inner
                    .send(Event::Control(crate::events::ControlEvent::Flatten))
                    .await
            }
            InstrumentFilter::One(legacy) => {
                let inst = crate::types::InstrumentId::new(legacy.exchange, legacy.symbol);
                let (tx, rx) = oneshot::channel();
                self.inner
                    .send_engine_command(crate::engine::EngineCommand::ClosePosition {
                        instrument: inst,
                        reply: tx,
                    })
                    .await?;
                let _ = rx.await;
                Ok(())
            }
        }
    }
}

impl<S, E> System<S, E>
where
    S: Strategy + Send + 'static,
    E: ExecutionGateway + 'static,
{
    /// Spawn engine tasks on runtime.
    pub async fn init_with_runtime(mut self, _rt: tokio::runtime::Handle) -> Result<Self> {
        let cfg = EngineConfig {
            channel_capacity: self.builder.channel_capacity,
            audit_broadcast_capacity: match self.builder.audit_mode {
                AuditMode::Enabled => Some(self.builder.audit_capacity),
                AuditMode::Disabled => None,
            },
            command_channel_capacity: self.builder.command_capacity,
            timer_schedules: self.builder.timer_schedules.clone(),
        };
        let strategy = self
            .strategy
            .take()
            .ok_or_else(|| crate::error::Error::Invalid("strategy already spawned".into()))?;
        let (handle, join, audit_rx) = EngineBuilder::spawn(
            cfg,
            self.state.clone(),
            strategy,
            self.risk.clone(),
            self.exec.clone(),
        );
        self.handle = handle;
        self.join = Some(join);
        self.audit_rx = audit_rx;
        Ok(self)
    }

    /// Control handle clone.
    pub fn handle(&self) -> SystemHandle {
        SystemHandle {
            inner: self.handle.clone(),
        }
    }

    /// Set trading state at runtime.
    pub async fn set_trading_state(&mut self, state: TradingState) {
        self.trading_state = state;
        self.state.lock().await.trading_state = state;
    }

    /// Take audit receiver.
    pub fn take_audit_rx(&mut self) -> Option<broadcast::Receiver<EngineAudit>> {
        self.audit_rx.take()
    }

    /// Shutdown: stop engine task.
    pub async fn shutdown(mut self) -> Result<(EngineSummary<S>, ShutdownAudit)> {
        drop(self.handle);
        if let Some(j) = self.join.take() {
            let _ = j.await;
        }
        let summary = EngineSummary {
            state: self.state.lock().await.clone(),
            strategy: self.strategy,
            equity_curve: self.equity_curve,
            config: self.config,
        };
        Ok((summary, ShutdownAudit))
    }

    /// Run iterator feed synchronously (backtest) using same code path as live.
    pub async fn run_iterator<I>(&mut self, events: I) -> Result<()>
    where
        I: IntoIterator<Item = Event>,
    {
        let strategy = self.strategy.as_mut().ok_or_else(|| {
            crate::error::Error::Invalid(
                "strategy moved to engine task; use Iterator feed without init".into(),
            )
        })?;
        for ev in events {
            let ts = event_timestamp(&ev);
            {
                let mut guard = self.state.lock().await;
                dispatch_event_audited(
                    &mut guard,
                    strategy,
                    &self.risk,
                    self.exec.as_ref(),
                    ev,
                    None,
                )
                .await?;
                if let Some(ref inst) = self.equity_instrument {
                    if let Some(eq) = guard.mark_to_market_equity(inst) {
                        self.equity_curve.push(EquityPoint {
                            ts,
                            equity_quote: eq,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn event_timestamp(ev: &Event) -> time::OffsetDateTime {
    ev.timestamp_or_now()
}

/// Engine retained after shutdown.
pub struct EngineSummary<S> {
    /// Final state.
    pub state: GlobalState,
    /// Strategy instance if not consumed by async engine task.
    pub strategy: Option<S>,
    /// Equity samples.
    pub equity_curve: Vec<EquityPoint>,
    /// Config.
    pub config: SystemConfig,
}

impl<S> EngineSummary<S> {
    /// Trading summary generator.
    pub fn trading_summary_generator(
        &self,
        risk_free_return: Decimal,
    ) -> TradingSummaryGenerator<'_> {
        TradingSummaryGenerator {
            equity: &self.equity_curve,
            risk_free_return,
        }
    }
}

/// Generates [`TradingSummary`] for a period.
pub struct TradingSummaryGenerator<'a> {
    equity: &'a [EquityPoint],
    risk_free_return: Decimal,
}

impl<'a> TradingSummaryGenerator<'a> {
    /// Build summary for period.
    pub fn generate(self, period: SummaryPeriod) -> TradingSummary {
        let rf = self
            .risk_free_return
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        TradingSummary::from_equity(period.label(), rf, self.equity.to_vec(), 252.0)
    }

    /// Print human-readable summary (barter-style).
    pub fn print_summary(self, period: SummaryPeriod) {
        self.generate(period).print_summary();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, MarketEvent};
    use crate::execution::{PaperConfig, SimGateway};
    use crate::instrument::{IndexedInstruments, SystemConfig};
    use crate::risk::DefaultRiskManager;
    use crate::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
    use crate::strategy::Strategy;
    use crate::types::{Asset, InstrumentId};
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::sync::Arc;
    use time::OffsetDateTime;

    struct Noop;
    impl Strategy for Noop {
        fn on_event(
            &mut self,
            _: &crate::strategy::StrategyContext,
            _: &Event,
            _: &mut Vec<crate::events::OrderIntent>,
        ) {
        }
    }

    #[tokio::test]
    async fn async_engine_updates_shared_state() {
        use crate::events::ControlEvent;

        let inst = InstrumentId::new("binance", "BTCUSDT");
        let mut map = HashMap::new();
        map.insert(
            inst.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut balances = HashMap::new();
        balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
        let state = GlobalState::new(InstrumentRegistry::from_instruments(map), balances);
        let config = SystemConfig {
            risk_free_return: Decimal::ZERO,
            instruments: vec![],
            executions: vec![],
        };
        let instruments = IndexedInstruments::new(vec![]);
        let risk: Arc<dyn RiskManager> = Arc::new(DefaultRiskManager::default());
        let exec = Arc::new(SimGateway::new(PaperConfig::default()));
        let args = SystemArgs::new(
            &instruments,
            config,
            Arc::new(LiveClock),
            Noop,
            risk,
            exec,
            state,
        );
        let mut system = SystemBuilder::new()
            .engine_feed_mode(EngineFeedMode::Async)
            .build(args)
            .expect("build");
        system = system
            .init_with_runtime(tokio::runtime::Handle::current())
            .await
            .expect("init");
        {
            let handle = system.handle();
            handle
                .send(Event::Control(ControlEvent::Pause))
                .await
                .expect("pause");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert!(system.state.lock().await.paused);
            handle
                .send(Event::Control(ControlEvent::Resume))
                .await
                .expect("resume");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert!(!system.state.lock().await.paused);
        }
        let (summary, _) = system.shutdown().await.expect("shutdown");
        assert!(!summary.state.paused);
    }

    #[tokio::test]
    async fn run_iterator_records_equity() {
        let inst = InstrumentId::new("binance", "BTCUSDT");
        let mut map = HashMap::new();
        map.insert(
            inst.clone(),
            InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
        );
        let mut balances = HashMap::new();
        balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
        let state = GlobalState::new(InstrumentRegistry::from_instruments(map), balances);
        let config = SystemConfig {
            risk_free_return: Decimal::ZERO,
            instruments: vec![],
            executions: vec![],
        };
        let instruments = IndexedInstruments::new(vec![]);
        let risk: Arc<dyn RiskManager> = Arc::new(DefaultRiskManager::default());
        let exec = Arc::new(SimGateway::new(PaperConfig::default()));
        let args = SystemArgs::new(
            &instruments,
            config,
            Arc::new(LiveClock),
            Noop,
            risk,
            exec,
            state,
        );
        let mut system = SystemBuilder::new()
            .engine_feed_mode(EngineFeedMode::Iterator)
            .equity_instrument(inst.clone())
            .build(args)
            .expect("build");

        let ts = OffsetDateTime::now_utc();
        let events: Vec<Event> = (0..3)
            .map(|_| {
                Event::Market(MarketEvent::BookL1 {
                    instrument: inst.clone(),
                    ts,
                    bid: Decimal::new(40_000, 0),
                    ask: Decimal::new(40_010, 0),
                })
            })
            .collect();
        system.run_iterator(events).await.expect("run");
        let (summary, _) = system.shutdown().await.expect("shutdown");
        assert_eq!(summary.equity_curve.len(), 3);
    }
}
