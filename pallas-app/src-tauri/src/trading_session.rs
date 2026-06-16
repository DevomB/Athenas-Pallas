use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use athenas_pallas::connectors::binance_spot::BinanceCombinedStream;
use athenas_pallas::connectors::MarketConnector;
use athenas_pallas::engine::{EngineBuilder, EngineCommand, EngineConfig, EngineHandle};
use athenas_pallas::events::{ControlEvent, Event, OrderIntent};
use athenas_pallas::execution::{BinanceCredentials, BinanceLiveGateway, PaperConfig, PaperGateway};
use athenas_pallas::risk::{PauseCheck, RiskPipeline};
use athenas_pallas::state::{GlobalState, InstrumentIndex, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, InstrumentId, TradingState};
use rust_decimal::Decimal;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::dto::{
    BalanceSnapshotDto, ConnectorStatusDto, FillEventDto, LiveSessionConfigDto,
    OpenOrderDto, PaperSessionConfigDto, PositionDto, PositionsSnapshotDto, TradingStateDto,
};
use athenas_pallas::connectors::binance_user_data::BinanceUserDataStream;

struct HoldStrategy;

impl Strategy for HoldStrategy {
    fn on_event(&mut self, _: &StrategyContext<'_>, _: &Event, _: &mut Vec<OrderIntent>) {}
}

enum SessionMode {
    Paper,
    Live,
}

struct ActiveSession {
    _runtime: tokio::runtime::Runtime,
    handle: EngineHandle,
    state: Arc<tokio::sync::Mutex<GlobalState>>,
    instrument: InstrumentId,
    mode: SessionMode,
    connected: Arc<std::sync::atomic::AtomicBool>,
    _shutdown_tx: tokio::sync::watch::Sender<bool>,
}

pub struct TradingSessionManager {
    inner: Mutex<Option<ActiveSession>>,
}

impl TradingSessionManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    pub fn trading_state(&self) -> TradingStateDto {
        let guard = self.inner.lock().unwrap();
        if let Some(s) = guard.as_ref() {
            let state = s.state.blocking_lock();
            TradingStateDto {
                mode: match s.mode {
                    SessionMode::Paper => "paper".into(),
                    SessionMode::Live => "live".into(),
                },
                instrument: format!("{}:{}", s.instrument.exchange, s.instrument.symbol),
                paused: state.paused,
                trading_enabled: state.trading_state == TradingState::Enabled,
                connected: s.connected.load(std::sync::atomic::Ordering::Relaxed),
            }
        } else {
            TradingStateDto {
                mode: "idle".into(),
                instrument: String::new(),
                paused: false,
                trading_enabled: true,
                connected: false,
            }
        }
    }

    pub fn snapshot(&self) -> Result<PositionsSnapshotDto, String> {
        let guard = self.inner.lock().unwrap();
        let s = guard.as_ref().ok_or_else(|| "no active trading session".to_string())?;
        let state = s.state.blocking_lock();
        let quote = Asset("USDT".into());
        let equity = state
            .portfolio_equity_for_quote(&quote)
            .to_string();
        let balances: Vec<BalanceSnapshotDto> = state
            .balances
            .iter()
            .map(|(a, v)| BalanceSnapshotDto {
                asset: a.0.to_string(),
                amount: v.to_string(),
            })
            .collect();
        let mut positions = Vec::new();
        for i in 0..state.registry.len() {
            let ix = InstrumentIndex(i);
            let qty = state.positions.get(i).copied().unwrap_or(Decimal::ZERO);
            if qty.is_zero() {
                continue;
            }
            let inst = state
                .registry
                .id(ix)
                .cloned()
                .unwrap_or_else(|| s.instrument.clone());
            let mark = state.mid_or_last_ix(i).map(|d| d.to_string());
            positions.push(PositionDto {
                instrument: format!("{}:{}", inst.exchange, inst.symbol),
                qty: qty.to_string(),
                mark_price: mark.clone(),
                notional: mark,
            });
        }
        Ok(PositionsSnapshotDto {
            balances,
            positions,
            equity,
            mark_price: state.mid_or_last_ix(0).map(|d| d.to_string()),
            paused: state.paused,
            trading_enabled: state.trading_state == TradingState::Enabled,
            connected: s.connected.load(std::sync::atomic::Ordering::Relaxed),
        })
    }

    pub fn start_paper(
        &self,
        app: AppHandle,
        config: PaperSessionConfigDto,
    ) -> Result<(), String> {
        self.stop()?;
        let instrument = InstrumentId::new(&config.exchange, &config.symbol);
        let (base, quote) = split_symbol(&config.symbol);
        let mut instruments = HashMap::new();
        instruments.insert(
            instrument.clone(),
            InstrumentMeta::spot(Asset(base.clone().into()), Asset(quote.clone().into())),
        );
        let mut balances = HashMap::new();
        let start_amt: Decimal = config
            .starting_balance_amount
            .parse()
            .map_err(|e: rust_decimal::Error| e.to_string())?;
        balances.insert(Asset(quote.into()), start_amt);
        balances.insert(Asset(base.into()), Decimal::ZERO);

        let registry = InstrumentRegistry::from_instruments(instruments);
        let state = Arc::new(tokio::sync::Mutex::new(GlobalState::new(registry, balances)));
        let fee = Decimal::from(config.fee_bps);
        let slip = Decimal::from(config.slippage_bps);
        let paper_cfg = PaperConfig {
            fee_bps: fee,
            market_slippage_bps: slip,
            fill_model: PaperConfig::default().fill_model,
        };
        let exec = Arc::new(PaperGateway::new(paper_cfg));
        let risk = RiskPipeline::new(vec![
            Box::new(PauseCheck::default()),
            Box::new(athenas_pallas::risk::TradingDisabledCheck::default()),
        ]);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);

        let (handle, _join, _audit) = EngineBuilder::spawn(
            EngineConfig {
                command_channel_capacity: Some(64),
                audit_broadcast_capacity: Some(256),
                ..EngineConfig::default()
            },
            state.clone(),
            HoldStrategy,
            risk,
            exec,
        );

        let stream_symbol = config.symbol.to_lowercase();
        let md_handle = handle.clone();
        let connected_flag = connected.clone();
        let inst = instrument.clone();
        let app_for_stream = app.clone();
        let inst_label = format!("{}:{}", instrument.exchange, instrument.symbol);
        runtime.spawn(async move {
            connected_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = app_for_stream.emit(
                "connector-status",
                ConnectorStatusDto {
                    status: "connected".into(),
                    instrument: inst_label.clone(),
                },
            );
            let connector = BinanceCombinedStream {
                instrument: inst,
                stream_symbol,
                ws_base: "wss://stream.binance.com:9443".into(),
            };
            if let Err(e) = connector.run(md_handle).await {
                connected_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                let _ = app_for_stream.emit(
                    "connector-status",
                    ConnectorStatusDto {
                        status: "disconnected".into(),
                        instrument: inst_label,
                    },
                );
                let _ = app_for_stream.emit("session-error", e.to_string());
            }
        });

        self.spawn_snapshot_task(
            &runtime,
            app.clone(),
            state.clone(),
            connected.clone(),
            instrument.clone(),
        );

        *self.inner.lock().unwrap() = Some(ActiveSession {
            _runtime: runtime,
            handle,
            state,
            instrument,
            mode: SessionMode::Paper,
            connected,
            _shutdown_tx: shutdown_tx,
        });
        let _ = app.emit("trading-state-changed", self.trading_state());
        let _ = app.emit("trading-session-started", ());
        Ok(())
    }

    pub fn start_live(
        &self,
        app: AppHandle,
        config: LiveSessionConfigDto,
        api_key: String,
        api_secret: String,
    ) -> Result<(), String> {
        self.stop()?;
        let (rest_base, ws_base) = if config.use_testnet {
            (
                "https://testnet.binance.vision".to_string(),
                "wss://testnet.binance.vision".to_string(),
            )
        } else {
            (
                "https://api.binance.com".to_string(),
                "wss://stream.binance.com:9443".to_string(),
            )
        };

        let instrument = InstrumentId::new(&config.exchange, &config.symbol);
        let (base, quote) = split_symbol(&config.symbol);
        let mut instruments = HashMap::new();
        instruments.insert(
            instrument.clone(),
            InstrumentMeta::spot(Asset(base.clone().into()), Asset(quote.clone().into())),
        );
        let mut balances = HashMap::new();
        balances.insert(Asset(quote.into()), Decimal::ZERO);
        balances.insert(Asset(base.into()), Decimal::ZERO);

        let registry = InstrumentRegistry::from_instruments(instruments);
        let state = Arc::new(tokio::sync::Mutex::new(GlobalState::new(registry, balances)));
        let risk = RiskPipeline::new(vec![
            Box::new(PauseCheck::default()),
            Box::new(athenas_pallas::risk::TradingDisabledCheck::default()),
        ]);
        let exec = Arc::new(BinanceLiveGateway::new(
            rest_base.clone(),
            BinanceCredentials {
                api_key: api_key.clone(),
                secret: api_secret.clone(),
            },
        ));

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);

        let (handle, _join, _audit) = EngineBuilder::spawn(
            EngineConfig {
                command_channel_capacity: Some(64),
                audit_broadcast_capacity: Some(256),
                ..EngineConfig::default()
            },
            state.clone(),
            HoldStrategy,
            risk,
            exec,
        );

        let md_handle = handle.clone();
        let connected_flag = connected.clone();
        let inst = instrument.clone();
        let stream_symbol = config.symbol.to_lowercase();
        let ws = ws_base.clone();
        let app_md = app.clone();
        let inst_label = format!("{}:{}", instrument.exchange, instrument.symbol);
        runtime.spawn(async move {
            connected_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = app_md.emit(
                "connector-status",
                ConnectorStatusDto {
                    status: "connected".into(),
                    instrument: inst_label.clone(),
                },
            );
            let connector = BinanceCombinedStream {
                instrument: inst.clone(),
                stream_symbol,
                ws_base: ws,
            };
            if let Err(e) = connector.run(md_handle).await {
                connected_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                let _ = app_md.emit(
                    "connector-status",
                    ConnectorStatusDto {
                        status: "disconnected".into(),
                        instrument: inst_label,
                    },
                );
                let _ = app_md.emit("session-error", e.to_string());
            }
        });

        let user_handle = handle.clone();
        let app_user = app.clone();
        runtime.spawn(async move {
            let user_stream = BinanceUserDataStream {
                api_key,
                rest_base,
                ws_base,
            };
            if let Err(e) = user_stream.run(user_handle).await {
                let _ = app_user.emit("session-error", e.to_string());
            }
        });

        self.spawn_snapshot_task(
            &runtime,
            app.clone(),
            state.clone(),
            connected.clone(),
            instrument.clone(),
        );

        *self.inner.lock().unwrap() = Some(ActiveSession {
            _runtime: runtime,
            handle,
            state,
            instrument,
            mode: SessionMode::Live,
            connected,
            _shutdown_tx: shutdown_tx,
        });
        let _ = app.emit("trading-state-changed", self.trading_state());
        let _ = app.emit("trading-session-started", ());
        Ok(())
    }

    fn spawn_snapshot_task(
        &self,
        runtime: &tokio::runtime::Runtime,
        app: AppHandle,
        state: Arc<tokio::sync::Mutex<GlobalState>>,
        connected: Arc<std::sync::atomic::AtomicBool>,
        instrument: InstrumentId,
    ) {
        runtime.spawn(async move {
            let mut last_fill_count = 0u64;
            let mut last_order_sig = String::new();
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let snap = {
                    let st = state.lock().await;
                    let quote = Asset("USDT".into());
                    let equity = st.portfolio_equity_for_quote(&quote).to_string();
                    let mark_price = st.mid_or_last_ix(0).map(|d| d.to_string());
                    let balances: Vec<BalanceSnapshotDto> = st
                        .balances
                        .iter()
                        .map(|(a, v)| BalanceSnapshotDto {
                            asset: a.0.to_string(),
                            amount: v.to_string(),
                        })
                        .collect();
                    let mut positions = Vec::new();
                    for i in 0..st.registry.len() {
                        let qty = st.positions.get(i).copied().unwrap_or(Decimal::ZERO);
                        if qty.is_zero() {
                            continue;
                        }
                        let inst = st
                            .registry
                            .id(InstrumentIndex(i))
                            .cloned()
                            .unwrap_or_else(|| instrument.clone());
                        positions.push(PositionDto {
                            instrument: format!("{}:{}", inst.exchange, inst.symbol),
                            qty: qty.to_string(),
                            mark_price: st.mid_or_last_ix(i).map(|d| d.to_string()),
                            notional: None,
                        });
                    }
                    let fill_count = st.fill_count;
                    let instrument_label = format!(
                        "{}:{}",
                        instrument.exchange, instrument.symbol
                    );
                    let new_fills: Vec<FillEventDto> = if fill_count > last_fill_count {
                        st.fill_log
                            .iter()
                            .skip(last_fill_count as usize)
                            .map(|f| FillEventDto {
                                ts: f.ts.to_string(),
                                instrument: instrument_label.clone(),
                                side: format!("{:?}", f.side),
                                qty: f.qty.clone(),
                                price: f.price.clone(),
                                fee: f.fee.clone(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    last_fill_count = fill_count;

                    let open_orders: Vec<OpenOrderDto> = st
                        .open_orders
                        .values()
                        .map(|o| OpenOrderDto {
                            id: format!("{:?}", o.id),
                            instrument: format!("{}:{}", o.instrument.exchange, o.instrument.symbol),
                            side: format!("{:?}", o.side),
                            order_type: format!("{:?}", o.order_type),
                            price: o.price.map(|d| d.to_string()),
                            stop_price: o.stop_price.map(|d| d.to_string()),
                            remaining_qty: o.remaining_qty.to_string(),
                            original_qty: o.original_qty.to_string(),
                            status: format!("{:?}", o.status),
                        })
                        .collect();
                    let order_sig = open_orders
                        .iter()
                        .map(|o| format!("{}:{}", o.id, o.remaining_qty))
                        .collect::<Vec<_>>()
                        .join("|");
                    let orders_changed = order_sig != last_order_sig;
                    last_order_sig = order_sig;

                    (
                        PositionsSnapshotDto {
                            balances,
                            positions,
                            equity,
                            mark_price,
                            paused: st.paused,
                            trading_enabled: st.trading_state == TradingState::Enabled,
                            connected: connected.load(std::sync::atomic::Ordering::Relaxed),
                        },
                        new_fills,
                        open_orders,
                        orders_changed,
                    )
                };
                let _ = app.emit("equity-tick", snap.0);
                for fill in snap.1 {
                    let _ = app.emit("fill", fill);
                }
                if snap.3 {
                    let _ = app.emit("order-update", snap.2);
                }
            }
        });
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        if guard.is_some() {
            *guard = None;
        }
        Ok(())
    }

    pub fn stop_with_emit(&self, app: &AppHandle) {
        let instrument = self
            .trading_state()
            .instrument
            .clone();
        let _ = self.stop();
        let _ = app.emit("trading-session-stopped", ());
        let _ = app.emit(
            "connector-status",
            ConnectorStatusDto {
                status: "disconnected".into(),
                instrument,
            },
        );
        let _ = app.emit(
            "trading-state-changed",
            TradingStateDto {
                mode: "idle".into(),
                instrument: String::new(),
                paused: false,
                trading_enabled: true,
                connected: false,
            },
        );
    }

    fn with_handle<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&EngineHandle) -> Result<R, String>,
    {
        let guard = self.inner.lock().unwrap();
        let s = guard.as_ref().ok_or_else(|| "no active trading session".to_string())?;
        f(&s.handle)
    }

    pub fn send_control(&self, app: &AppHandle, evt: ControlEvent) -> Result<(), String> {
        self.with_handle(|h| {
            h.try_send(Event::Control(evt))
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        let _ = app.emit("trading-state-changed", self.trading_state());
        Ok(())
    }

    pub fn cancel_all(&self) -> Result<(), String> {
        self.with_handle(|h| {
            h.try_send(Event::Control(ControlEvent::CancelAll))
                .map_err(|e| e.to_string())
        })
    }

    pub fn flatten_all(&self) -> Result<(), String> {
        self.with_handle(|h| {
            h.try_send(Event::Control(ControlEvent::Flatten))
                .map_err(|e| e.to_string())
        })
    }

    pub fn list_open_orders(&self) -> Result<Vec<athenas_pallas::types::OpenOrder>, String> {
        self.with_handle(|h| {
            let (tx, rx) = oneshot::channel();
            h.try_send_engine_command(EngineCommand::ListOpenOrders(tx))
                .map_err(|e| e.to_string())?;
            rx.blocking_recv()
                .map_err(|_| "engine command channel closed".to_string())
        })
    }
}

impl Default for TradingSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

fn split_symbol(symbol: &str) -> (String, String) {
    for quote in ["USDT", "USDC", "BUSD", "USD", "BTC", "ETH"] {
        if let Some(base) = symbol.strip_suffix(quote) {
            if !base.is_empty() {
                return (base.to_string(), quote.to_string());
            }
        }
    }
    (symbol.to_string(), "USDT".to_string())
}
