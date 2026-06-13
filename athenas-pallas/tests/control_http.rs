#![cfg(feature = "control-server")]

use athenas_pallas::control::{serve, ControlServerConfig};
use athenas_pallas::engine::{EngineBuilder, EngineConfig};
use athenas_pallas::execution::{PaperConfig, PaperGateway};
use athenas_pallas::instrument::InstrumentRegistry;
use athenas_pallas::risk::RiskPipeline;
use athenas_pallas::state::GlobalState;
use athenas_pallas::strategy::Strategy;
use std::collections::HashMap;
use std::sync::Arc;

struct Noop;
impl Strategy for Noop {
    fn on_event(
        &mut self,
        _ctx: &athenas_pallas::strategy::StrategyContext<'_>,
        _ev: &athenas_pallas::events::Event,
        _: &mut Vec<athenas_pallas::events::OrderIntent>,
    ) {
    }
}

#[tokio::test]
async fn pause_endpoint_returns_success() {
    let state = GlobalState::new(
        InstrumentRegistry::from_instruments(HashMap::new()),
        HashMap::new(),
    );
    let exec = Arc::new(PaperGateway::new(PaperConfig::default()));
    let (handle, _join, _) = EngineBuilder::spawn(
        EngineConfig {
            channel_capacity: 8,
            audit_broadcast_capacity: None,
            command_channel_capacity: None,
            timer_schedules: vec![],
        },
        Arc::new(tokio::sync::Mutex::new(state)),
        Noop,
        RiskPipeline::new(vec![]),
        exec,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let cfg = ControlServerConfig {
        bind: addr.to_string(),
        secret: "test".into(),
    };
    let server = tokio::spawn(async move { serve(handle, cfg).await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/pause"))
        .header("x-pallas-secret", "test")
        .send()
        .await
        .expect("request");
    assert!(res.status().is_success());
    server.abort();
}

#[tokio::test]
async fn resume_endpoint_returns_success() {
    let state = GlobalState::new(
        InstrumentRegistry::from_instruments(HashMap::new()),
        HashMap::new(),
    );
    let exec = Arc::new(PaperGateway::new(PaperConfig::default()));
    let (handle, _join, _) = EngineBuilder::spawn(
        EngineConfig {
            channel_capacity: 8,
            audit_broadcast_capacity: None,
            command_channel_capacity: None,
            timer_schedules: vec![],
        },
        Arc::new(tokio::sync::Mutex::new(state)),
        Noop,
        RiskPipeline::new(vec![]),
        exec,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let cfg = ControlServerConfig {
        bind: addr.to_string(),
        secret: "test".into(),
    };
    let server = tokio::spawn(async move { serve(handle, cfg).await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/resume"))
        .header("x-pallas-secret", "test")
        .send()
        .await
        .expect("request");
    assert!(res.status().is_success());
    server.abort();
}
