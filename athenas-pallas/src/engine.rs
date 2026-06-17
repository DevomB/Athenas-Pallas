//! Engine: single consumer loop - market -> (passive fills) -> strategy -> risk -> execution.

use crate::audit::{self, EngineAudit, StrategySkipReason};
use crate::error::{Error, Result};
use crate::events::{ControlEvent, Event, OrderIntent, OrderIntentSource, TimerEvent};
use crate::execution::{ExecutionGateway, SyncExecutionGateway};
use crate::risk::RiskPipeline;
use crate::state::{GlobalState, InstrumentIndex};
use crate::strategy::{Strategy, StrategyContext};
use crate::types::{InstrumentId, OpenOrder, OrderType, Side, TradingState};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tracing::{error, warn};

/// Side-channel command with reply (requires [`EngineConfig::command_channel_capacity`]).
pub enum EngineCommand {
    /// Return a snapshot of all working orders (same view as [`GlobalState::open_orders`]).
    ListOpenOrders(oneshot::Sender<Vec<OpenOrder>>),
    /// Cancel every open order for `instrument` (venue round-trips via [`ExecutionGateway::cancel`]).
    CancelOrdersInstrument {
        /// Target pair.
        instrument: InstrumentId,
        /// Number of cancel calls that returned Ok (one per order id).
        reply: oneshot::Sender<std::result::Result<usize, String>>,
    },
    /// Cancel working orders for `instrument`, then submit a flattening **market** intent (same path as control flatten).
    ClosePosition {
        /// Target pair.
        instrument: InstrumentId,
        /// Ok when flat or already flat; Err when risk or execution fails.
        reply: oneshot::Sender<std::result::Result<(), String>>,
    },
}

/// Handle to inject events from connectors or control plane.
#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<Event>,
    cmd_tx: Option<mpsc::Sender<EngineCommand>>,
}

impl EngineHandle {
    /// Placeholder before [`crate::system::System::init_with_runtime`] wires a live channel.
    pub fn disconnected() -> Self {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        Self { tx, cmd_tx: None }
    }

    /// Clone the underlying sender (for connectors / control fan-in).
    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.tx.clone()
    }

    /// Send one event into the engine (non-blocking).
    pub fn try_send(&self, event: Event) -> Result<()> {
        self.tx.try_send(event).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => Error::EventDropped,
            mpsc::error::TrySendError::Closed(_) => Error::EngineShutdown,
        })
    }

    /// Async send.
    pub async fn send(&self, event: Event) -> Result<()> {
        self.tx.send(event).await.map_err(|_| Error::EngineShutdown)
    }

    /// Queue a command when [`EngineConfig::command_channel_capacity`] was set at spawn.
    pub fn try_send_engine_command(&self, cmd: EngineCommand) -> Result<()> {
        let c = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::Invalid("engine commands not enabled".into()))?;
        c.try_send(cmd).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => Error::EventDropped,
            mpsc::error::TrySendError::Closed(_) => Error::EngineShutdown,
        })
    }

    /// Async queue for engine commands.
    pub async fn send_engine_command(&self, cmd: EngineCommand) -> Result<()> {
        let c = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::Invalid("engine commands not enabled".into()))?;
        c.send(cmd).await.map_err(|_| Error::EngineShutdown)
    }
}

/// Wall-clock timer schedule injected by [`EngineBuilder::spawn`].
#[derive(Clone, Debug)]
pub struct TimerSchedule {
    /// Tick interval.
    pub interval: Duration,
    /// Identifier delivered on each [`crate::events::TimerEvent`].
    pub id: u32,
}

/// Engine wiring (channel capacity, optional periodic timers, optional audit broadcast).
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Channel capacity.
    pub channel_capacity: usize,
    /// Interval timers forwarding [`Event::Timer`] into the engine loop.
    pub timer_schedules: Vec<TimerSchedule>,
    /// When `Some(cap)`, [`EngineBuilder::spawn`] returns a [`broadcast::Receiver`] of [`EngineAudit`](crate::audit::EngineAudit) with lagging subscribers dropped at capacity.
    pub audit_broadcast_capacity: Option<usize>,
    /// When `Some(cap)`, [`EngineHandle::send_engine_command`] is available and processed on the engine task.
    pub command_channel_capacity: Option<usize>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 1024,
            timer_schedules: Vec::new(),
            audit_broadcast_capacity: None,
            command_channel_capacity: None,
        }
    }
}

pub(crate) fn spawn_timer_tasks(handle: EngineHandle, schedules: Vec<TimerSchedule>) {
    for s in schedules {
        let h = handle.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(s.interval);
            loop {
                interval.tick().await;
                let ts = OffsetDateTime::now_utc();
                let _ = h.send(Event::Timer(TimerEvent { ts, id: s.id })).await;
            }
        });
    }
}

/// Builder for spawning the engine task.
pub struct EngineBuilder;

impl EngineBuilder {
    /// Spawn background consumer. Returns control handle, join handle, and optional audit receiver.
    pub fn spawn<S, E>(
        cfg: EngineConfig,
        state: Arc<Mutex<GlobalState>>,
        mut strategy: S,
        risk: RiskPipeline,
        exec: Arc<E>,
    ) -> (
        EngineHandle,
        tokio::task::JoinHandle<Result<()>>,
        Option<broadcast::Receiver<EngineAudit>>,
    )
    where
        S: Strategy + Send + 'static,
        E: ExecutionGateway + 'static,
    {
        let (tx, mut rx) = mpsc::channel(cfg.channel_capacity);
        let cmd_channel = match cfg.command_channel_capacity {
            Some(c) if c > 0 => {
                let (t, r) = mpsc::channel(c);
                Some((t, r))
            }
            _ => None,
        };
        let cmd_tx = cmd_channel.as_ref().map(|(t, _)| t.clone());
        let cmd_rx_opt = cmd_channel.map(|(_, r)| r);
        let handle = EngineHandle { tx, cmd_tx };
        let (audit_tx, audit_rx) = match cfg.audit_broadcast_capacity {
            Some(cap) if cap > 0 => {
                let (t, r) = broadcast::channel(cap);
                (Some(Arc::new(t)), Some(r))
            }
            _ => (None, None),
        };
        spawn_timer_tasks(handle.clone(), cfg.timer_schedules.clone());
        let join = tokio::spawn(async move {
            let audit_ref = audit_tx.as_deref();
            if let Some(mut cmd_rx) = cmd_rx_opt {
                loop {
                    tokio::select! {
                        ev = rx.recv() => {
                            match ev {
                                None => break,
                                Some(ev) => {
                                    let mut guard = state.lock().await;
                                    if let Err(e) = process_event(
                                        &mut guard,
                                        &mut strategy,
                                        &risk,
                                        exec.as_ref(),
                                        ev,
                                        audit_ref,
                                    )
                                    .await
                                    {
                                        warn!(target: "athenas_pallas::engine", "engine step: {e}");
                                    }
                                }
                            }
                        }
                        cmd = cmd_rx.recv() => {
                            match cmd {
                                None => {}
                                Some(cmd) => {
                                    let mut guard = state.lock().await;
                                    handle_engine_command(
                                        &mut guard,
                                        exec.as_ref(),
                                        &risk,
                                        cmd,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }
            } else {
                while let Some(ev) = rx.recv().await {
                    let mut guard = state.lock().await;
                    if let Err(e) = process_event(
                        &mut guard,
                        &mut strategy,
                        &risk,
                        exec.as_ref(),
                        ev,
                        audit_ref,
                    )
                    .await
                    {
                        warn!(target: "athenas_pallas::engine", "engine step: {e}");
                    }
                }
            }
            Ok(())
        });
        (handle, join, audit_rx)
    }

    /// Offline replay helper (CSV / synthetic) without background tasks.
    pub async fn run_batch<S, E>(
        mut state: GlobalState,
        mut strategy: S,
        risk: &RiskPipeline,
        exec: &E,
        events: Vec<Event>,
    ) -> Result<GlobalState>
    where
        S: Strategy,
        E: ExecutionGateway,
    {
        for ev in events {
            process_event(&mut state, &mut strategy, risk, exec, ev, None).await?;
        }
        Ok(state)
    }
}

/// One deterministic engine iteration (backtests / unit tests).
pub async fn engine_step<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
) -> Result<()>
where
    S: Strategy,
    E: ExecutionGateway,
{
    process_event(state, strategy, risk, exec, ev, None).await
}

/// Legacy type alias - prefer [`EngineBuilder::spawn`].
pub type Engine = EngineBuilder;

/// Process a single event with optional audit broadcast (replicas, logging).
pub async fn dispatch_event_audited<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
    audit: Option<&broadcast::Sender<EngineAudit>>,
) -> Result<()>
where
    S: Strategy,
    E: ExecutionGateway,
{
    process_event(state, strategy, risk, exec, ev, audit).await
}

/// Process a single event (market/account/control/timer) through strategy -> risk -> execution.
/// Use this in backtests or tests; live engines use [`EngineBuilder::spawn`].
pub async fn dispatch_event<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
) -> Result<()>
where
    S: Strategy,
    E: ExecutionGateway,
{
    dispatch_event_audited(state, strategy, risk, exec, ev, None).await
}

/// Sync event dispatch for backtest replay (no async executor on the hot path).
pub fn dispatch_event_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    process_event_sync(state, strategy, risk, exec, ev, intents)
}

fn process_event_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    match &ev {
        Event::Market(m) => {
            state.apply_market(m);
            let passive = exec.poll_after_market(state)?;
            for a in passive {
                state.apply_account(&a);
            }
        }
        Event::Account(a) => state.apply_account(a),
        Event::Control(c) => apply_control_sync(state, exec, risk, c)?,
        Event::Timer(_) => {}
    }

    dispatch_strategy_sync(state, strategy, risk, exec, &ev, intents)
}

/// Run strategy, risk, and execution against live state (no snapshot clones).
pub fn dispatch_strategy_sync<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: &Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
    E: SyncExecutionGateway,
{
    let now = event_time(ev);
    state.refresh_daily_risk_anchor(now);

    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }

    let ctx = StrategyContext { now, state };
    intents.clear();
    strategy.on_event(&ctx, ev, intents);

    for intent in intents.drain(..) {
        if let Err(e) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {e}");
            continue;
        }
        match dispatch_intent_sync(exec, state, &intent) {
            Ok(evs) => {
                for a in evs {
                    state.apply_account(&a);
                }
            }
            Err(e) => error!(target: "athenas_pallas::engine", "execution: {e}"),
        }
    }

    Ok(())
}

/// Backtest replay: market already applied; inline risk checks.
pub fn dispatch_replay_sync<S>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &crate::risk::BacktestChecks,
    exec: &impl SyncExecutionGateway,
    ev: Event,
    intents: &mut Vec<OrderIntent>,
) -> Result<()>
where
    S: Strategy,
{
    match &ev {
        Event::Market(_) => {
            let passive = exec.poll_after_market(state)?;
            for a in passive {
                state.apply_account(&a);
            }
        }
        Event::Account(a) => state.apply_account(a),
        Event::Control(_) | Event::Timer(_) => {}
    }

    let now = event_time(&ev);
    state.refresh_daily_risk_anchor(now);

    if state.paused || state.trading_state == TradingState::Disabled {
        return Ok(());
    }

    let ctx = StrategyContext { now, state };
    intents.clear();
    strategy.on_event(&ctx, &ev, intents);

    for intent in intents.drain(..) {
        if let Err(e) = risk.validate(state, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {e}");
            continue;
        }
        match dispatch_intent_sync(exec, state, &intent) {
            Ok(evs) => {
                for a in evs {
                    state.apply_account(&a);
                }
            }
            Err(e) => error!(target: "athenas_pallas::engine", "execution: {e}"),
        }
    }

    Ok(())
}

fn apply_control_sync<E: SyncExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskPipeline,
    c: &ControlEvent,
) -> Result<()> {
    match c {
        ControlEvent::Pause => state.paused = true,
        ControlEvent::Resume => state.paused = false,
        ControlEvent::DisableTrading => state.trading_state = TradingState::Disabled,
        ControlEvent::EnableTrading => state.trading_state = TradingState::Enabled,
        ControlEvent::CancelAll => {
            let evs = exec.cancel_all(&state.snapshot())?;
            for a in evs {
                state.apply_account(&a);
            }
        }
        ControlEvent::Flatten => {
            let evs = exec.cancel_all(&state.snapshot())?;
            for a in evs {
                state.apply_account(&a);
            }
            let insts: Vec<_> = state
                .positions
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.is_zero())
                .filter_map(|(ix, _)| state.registry.id(InstrumentIndex(ix)).cloned())
                .collect();
            for inst in insts {
                close_position_with_flatten_source_sync(state, exec, risk, inst);
            }
        }
    }
    Ok(())
}

fn close_position_with_flatten_source_sync<E: SyncExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskPipeline,
    inst: InstrumentId,
) {
    let snap = state.snapshot();
    let pos = snap.position_qty(&inst);
    if pos.is_zero() {
        return;
    }
    let intent = OrderIntent {
        instrument: inst,
        side: if pos > Decimal::ZERO {
            Side::Sell
        } else {
            Side::Buy
        },
        order_type: OrderType::Market,
        price: None,
        stop_price: None,
        qty: pos.abs(),
        client_order_id: None,
        source: OrderIntentSource::Flatten,
        strategy_id: None,
    };
    if let Err(e) = risk.validate(&snap, &intent) {
        warn!(target: "athenas_pallas::engine", "flatten risk: {e}");
        return;
    }
    match dispatch_intent_sync(exec, &snap, &intent) {
        Ok(evs) => {
            for a in evs {
                state.apply_account(&a);
            }
        }
        Err(e) => error!(target: "athenas_pallas::engine", "flatten execution: {e}"),
    }
}

fn dispatch_intent_sync<E: SyncExecutionGateway>(
    exec: &E,
    state: &GlobalState,
    intent: &OrderIntent,
) -> Result<Vec<crate::events::AccountEvent>> {
    match intent.order_type {
        OrderType::Limit => exec.place_limit(state, intent),
        OrderType::Market => exec.place_market(state, intent),
        OrderType::StopMarket => exec.place_stop_market(state, intent),
        OrderType::StopLimit => exec.place_stop_limit(state, intent),
    }
}

async fn process_event<S, E>(
    state: &mut GlobalState,
    strategy: &mut S,
    risk: &RiskPipeline,
    exec: &E,
    ev: Event,
    audit: Option<&broadcast::Sender<EngineAudit>>,
) -> Result<()>
where
    S: Strategy,
    E: ExecutionGateway,
{
    if let Some(tx) = audit {
        audit::emit_event_ingested(tx, &ev);
    }

    match &ev {
        Event::Market(m) => {
            state.apply_market(m);
            let passive = exec.poll_after_market(&state.snapshot()).await?;
            for a in passive {
                state.apply_account(&a);
            }
        }
        Event::Account(a) => state.apply_account(a),
        Event::Control(c) => {
            apply_control(state, exec, risk, c).await?;
            if let Some(tx) = audit {
                audit::try_emit(tx, EngineAudit::ControlApplied { control: c.into() });
            }
        }
        Event::Timer(_) => {}
    }

    let now = event_time(&ev);
    state.refresh_daily_risk_anchor(now);

    if state.paused {
        if let Some(tx) = audit {
            audit::try_emit(
                tx,
                EngineAudit::StrategySkipped {
                    reason: StrategySkipReason::Paused,
                },
            );
        }
        return Ok(());
    }
    if state.trading_state == TradingState::Disabled {
        if let Some(tx) = audit {
            audit::try_emit(
                tx,
                EngineAudit::StrategySkipped {
                    reason: StrategySkipReason::TradingDisabled,
                },
            );
        }
        return Ok(());
    }

    let snap = state.snapshot();
    let ctx = StrategyContext { now, state: &snap };
    let mut intents = Vec::new();
    strategy.on_event(&ctx, &ev, &mut intents);

    for intent in intents {
        let snap = state.snapshot();
        if let Err(e) = risk.validate(&snap, &intent) {
            warn!(target: "athenas_pallas::engine", "risk: {e}");
            if let Some(tx) = audit {
                audit::try_emit(
                    tx,
                    EngineAudit::RiskRejected {
                        intent: intent.clone(),
                        message: e.to_string(),
                    },
                );
            }
            continue;
        }
        let events = dispatch_intent(exec, &snap, &intent).await;
        match events {
            Ok(evs) => {
                if let Some(tx) = audit {
                    if !evs.is_empty() {
                        audit::try_emit(
                            tx,
                            EngineAudit::IntentsExecuted {
                                account_events: evs.len(),
                            },
                        );
                    }
                }
                for a in evs {
                    state.apply_account(&a);
                }
            }
            Err(e) => {
                error!(target: "athenas_pallas::engine", "execution: {e}");
                if let Some(tx) = audit {
                    audit::try_emit(
                        tx,
                        EngineAudit::ExecutionError {
                            message: e.to_string(),
                        },
                    );
                }
            }
        }
    }

    Ok(())
}

fn event_time(ev: &Event) -> OffsetDateTime {
    match ev {
        Event::Market(crate::events::MarketEvent::Trade { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::BookL1 { ts, .. }) => *ts,
        Event::Market(crate::events::MarketEvent::BookL2Snapshot(s)) => s.ts,
        Event::Market(crate::events::MarketEvent::Bar { ts, .. }) => *ts,
        Event::Timer(t) => t.ts,
        _ => OffsetDateTime::now_utc(),
    }
}

async fn apply_control<E: ExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskPipeline,
    c: &ControlEvent,
) -> Result<()> {
    match c {
        ControlEvent::Pause => state.paused = true,
        ControlEvent::Resume => state.paused = false,
        ControlEvent::DisableTrading => state.trading_state = TradingState::Disabled,
        ControlEvent::EnableTrading => state.trading_state = TradingState::Enabled,
        ControlEvent::CancelAll => {
            let evs = exec.cancel_all(&state.snapshot()).await?;
            for a in evs {
                state.apply_account(&a);
            }
        }
        ControlEvent::Flatten => {
            let evs = exec.cancel_all(&state.snapshot()).await?;
            for a in evs {
                state.apply_account(&a);
            }
            let insts: Vec<_> = state
                .positions
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.is_zero())
                .filter_map(|(ix, _)| state.registry.id(InstrumentIndex(ix)).cloned())
                .collect();
            for inst in insts {
                close_position_with_flatten_source(state, exec, risk, inst).await;
            }
        }
    }
    Ok(())
}

async fn close_position_with_flatten_source<E: ExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskPipeline,
    inst: InstrumentId,
) {
    let snap = state.snapshot();
    let pos = snap.position_qty(&inst);
    if pos.is_zero() {
        return;
    }
    let intent = OrderIntent {
        instrument: inst,
        side: if pos > Decimal::ZERO {
            Side::Sell
        } else {
            Side::Buy
        },
        order_type: OrderType::Market,
        price: None,
        stop_price: None,
        qty: pos.abs(),
        client_order_id: None,
        source: OrderIntentSource::Flatten,
        strategy_id: None,
    };
    if let Err(e) = risk.validate(&snap, &intent) {
        warn!(target: "athenas_pallas::engine", "flatten risk: {e}");
        return;
    }
    match dispatch_intent(exec, &snap, &intent).await {
        Ok(evs) => {
            for a in evs {
                state.apply_account(&a);
            }
        }
        Err(e) => error!(target: "athenas_pallas::engine", "flatten execution: {e}"),
    }
}

async fn cancel_open_orders_for_instrument<E: ExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    instrument: &InstrumentId,
) -> std::result::Result<(), String> {
    let ids: Vec<_> = state
        .open_orders
        .values()
        .filter(|o| o.instrument == *instrument)
        .map(|o| o.id.clone())
        .collect();
    for oid in ids {
        let snap = state.snapshot();
        let evs = exec.cancel(&snap, oid).await.map_err(|e| e.to_string())?;
        for a in evs {
            state.apply_account(&a);
        }
    }
    Ok(())
}

async fn handle_engine_command<E: ExecutionGateway>(
    state: &mut GlobalState,
    exec: &E,
    risk: &RiskPipeline,
    cmd: EngineCommand,
) {
    match cmd {
        EngineCommand::ListOpenOrders(reply) => {
            let v: Vec<OpenOrder> = state.open_orders.values().cloned().collect();
            let _ = reply.send(v);
        }
        EngineCommand::CancelOrdersInstrument { instrument, reply } => {
            let ids: Vec<_> = state
                .open_orders
                .values()
                .filter(|o| o.instrument == instrument)
                .map(|o| o.id.clone())
                .collect();
            let mut canceled = 0usize;
            for oid in ids {
                let snap = state.snapshot();
                match exec.cancel(&snap, oid).await {
                    Ok(evs) => {
                        canceled += 1;
                        for a in evs {
                            state.apply_account(&a);
                        }
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e.to_string()));
                        return;
                    }
                }
            }
            let _ = reply.send(Ok(canceled));
        }
        EngineCommand::ClosePosition { instrument, reply } => {
            if let Err(e) = cancel_open_orders_for_instrument(state, exec, &instrument).await {
                let _ = reply.send(Err(e));
                return;
            }
            let snap = state.snapshot();
            let pos = snap.position_qty(&instrument);
            if pos.is_zero() {
                let _ = reply.send(Ok(()));
                return;
            }
            let intent = OrderIntent {
                instrument: instrument.clone(),
                side: if pos > Decimal::ZERO {
                    Side::Sell
                } else {
                    Side::Buy
                },
                order_type: OrderType::Market,
                price: None,
                stop_price: None,
                qty: pos.abs(),
                client_order_id: None,
                source: OrderIntentSource::Flatten,
                strategy_id: None,
            };
            if let Err(e) = risk.validate(&snap, &intent) {
                let _ = reply.send(Err(e.to_string()));
                return;
            }
            match dispatch_intent(exec, &snap, &intent).await {
                Ok(evs) => {
                    for a in evs {
                        state.apply_account(&a);
                    }
                    let _ = reply.send(Ok(()));
                }
                Err(e) => {
                    let _ = reply.send(Err(e.to_string()));
                }
            }
        }
    }
}

async fn dispatch_intent<E: ExecutionGateway>(
    exec: &E,
    state: &GlobalState,
    intent: &OrderIntent,
) -> Result<Vec<crate::events::AccountEvent>> {
    match intent.order_type {
        OrderType::Limit => exec.place_limit(state, intent).await,
        OrderType::Market => exec.place_market(state, intent).await,
        OrderType::StopMarket => exec.place_stop_market(state, intent).await,
        OrderType::StopLimit => exec.place_stop_limit(state, intent).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, TimerEvent};
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn timer_schedule_emits_id() {
        let (tx, mut rx) = mpsc::channel(16);
        let handle = EngineHandle { tx, cmd_tx: None };
        spawn_timer_tasks(
            handle,
            vec![TimerSchedule {
                interval: Duration::from_millis(25),
                id: 7,
            }],
        );
        let mut got = None;
        for _ in 0..80 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            while let Ok(ev) = rx.try_recv() {
                if let Event::Timer(TimerEvent { id, .. }) = ev {
                    got = Some(id);
                }
            }
            if got.is_some() {
                break;
            }
        }
        assert_eq!(got, Some(7));
    }
}
