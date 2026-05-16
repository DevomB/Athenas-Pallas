//! Localhost HTTP control plane (`control-server` feature).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;
use tokio::sync::oneshot;
use tracing::info;

use crate::engine::{EngineCommand, EngineHandle};
use crate::events::{ControlEvent, Event};
use crate::types::{InstrumentId, OpenOrder};

/// Header checked when a secret is configured (`x-pallas-secret`).
pub const HEADER_SECRET: &str = "x-pallas-secret";

/// HTTP bind target and shared secret.
#[derive(Clone, Debug)]
pub struct ControlServerConfig {
    /// e.g. `127.0.0.1:9847`
    pub bind: String,
    /// Required header value for `HEADER_SECRET`.
    pub secret: String,
}

struct Ctx {
    handle: EngineHandle,
    secret: String,
}

fn authorize(headers: &HeaderMap, secret: &str) -> bool {
    headers
        .get(HEADER_SECRET)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == secret)
}

/// Serve until the process stops or the socket errors.
pub async fn serve(handle: EngineHandle, cfg: ControlServerConfig) -> crate::error::Result<()> {
    let ctx = Arc::new(Ctx {
        handle,
        secret: cfg.secret,
    });
    let app = Router::new()
        .route("/pause", post(pause_handler))
        .route("/resume", post(resume_handler))
        .route("/trading-disable", post(trading_disable_handler))
        .route("/trading-enable", post(trading_enable_handler))
        .route("/cancel-all", post(cancel_all_handler))
        .route("/flatten", post(flatten_handler))
        .route("/open-orders", get(open_orders_handler))
        .route("/cancel-instrument", post(cancel_instrument_handler))
        .route("/close-position", post(close_position_handler))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    info!(target: "athenas_pallas::control", "listening on {}", cfg.bind);
    axum::serve(listener, app)
        .await
        .map_err(|e| crate::error::Error::Invalid(e.to_string()))?;
    Ok(())
}

async fn pause_handler(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> StatusCode {
    if !authorize(&headers, &ctx.secret) {
        return StatusCode::UNAUTHORIZED;
    }
    match ctx
        .handle
        .send(Event::Control(ControlEvent::Pause))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn resume_handler(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> StatusCode {
    if !authorize(&headers, &ctx.secret) {
        return StatusCode::UNAUTHORIZED;
    }
    match ctx
        .handle
        .send(Event::Control(ControlEvent::Resume))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn trading_disable_handler(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> StatusCode {
    if !authorize(&headers, &ctx.secret) {
        return StatusCode::UNAUTHORIZED;
    }
    match ctx
        .handle
        .send(Event::Control(ControlEvent::DisableTrading))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn trading_enable_handler(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> StatusCode {
    if !authorize(&headers, &ctx.secret) {
        return StatusCode::UNAUTHORIZED;
    }
    match ctx
        .handle
        .send(Event::Control(ControlEvent::EnableTrading))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn cancel_all_handler(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> StatusCode {
    if !authorize(&headers, &ctx.secret) {
        return StatusCode::UNAUTHORIZED;
    }
    match ctx
        .handle
        .send(Event::Control(ControlEvent::CancelAll))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn flatten_handler(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> StatusCode {
    if !authorize(&headers, &ctx.secret) {
        return StatusCode::UNAUTHORIZED;
    }
    match ctx
        .handle
        .send(Event::Control(ControlEvent::Flatten))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn open_orders_handler(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
) -> std::result::Result<Json<Vec<OpenOrder>>, StatusCode> {
    if !authorize(&headers, &ctx.secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let (tx, rx) = oneshot::channel();
    ctx.handle
        .send_engine_command(EngineCommand::ListOpenOrders(tx))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let orders = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(orders))
}

#[derive(Deserialize)]
struct InstrumentBody {
    instrument: InstrumentId,
}

async fn cancel_instrument_handler(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    Json(body): Json<InstrumentBody>,
) -> std::result::Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    if !authorize(&headers, &ctx.secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let (tx, rx) = oneshot::channel();
    ctx.handle
        .send_engine_command(EngineCommand::CancelOrdersInstrument {
            instrument: body.instrument,
            reply: tx,
        })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let res = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match res {
        Ok(n) => Ok((StatusCode::OK, Json(serde_json::json!({ "canceled": n })))),
        Err(msg) => Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": msg })),
        )),
    }
}

async fn close_position_handler(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    Json(body): Json<InstrumentBody>,
) -> std::result::Result<StatusCode, StatusCode> {
    if !authorize(&headers, &ctx.secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let (tx, rx) = oneshot::channel();
    ctx.handle
        .send_engine_command(EngineCommand::ClosePosition {
            instrument: body.instrument,
            reply: tx,
        })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let res = rx.await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match res {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Ok(StatusCode::UNPROCESSABLE_ENTITY),
    }
}
