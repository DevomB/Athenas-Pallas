//! Paper trading example: live Binance **public** market data + local paper execution.
//!
//! ```text
//! cargo run -p athenas-pallas --example paper_binance --features binance,control-server
//! ```
//!
//! Optional: `PALLAS_CONTROL_TOKEN` (default `dev`) for `POST http://127.0.0.1:9847/pause` etc.
//! Send header `x-pallas-secret: <token>`.

use athenas_pallas::connectors::binance_spot::BinanceCombinedStream;
use athenas_pallas::connectors::MarketConnector;
use athenas_pallas::control::{serve, ControlServerConfig, HEADER_SECRET};
use athenas_pallas::engine::{EngineBuilder, EngineConfig};
use athenas_pallas::events::{Event, MarketEvent, OrderIntent};
use athenas_pallas::execution::{PaperConfig, PaperGateway};
use athenas_pallas::risk::{PauseCheck, RiskPipeline};
use athenas_pallas::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, InstrumentId, OrderType, Side};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

struct DemoStrategy {
    instrument: InstrumentId,
    fired: bool,
}

impl Strategy for DemoStrategy {
    fn on_event(&mut self, ctx: &StrategyContext<'_>, event: &Event, out: &mut Vec<OrderIntent>) {
        if self.fired {
            return;
        }
        if let Event::Market(MarketEvent::BookL1 { instrument, .. }) = event {
            if instrument != &self.instrument {
                return;
            }
            if ctx.state.mid_or_last(&self.instrument).is_none() {
                return;
            }
            self.fired = true;
            let qty = Decimal::from_f64(0.0001).unwrap_or(Decimal::ZERO);
            if qty.is_zero() {
                return;
            }
            info!("submitting one small paper MARKET buy");
            out.push(OrderIntent {
                instrument: self.instrument.clone(),
                side: Side::Buy,
                order_type: OrderType::Market,
                price: None,
                qty,
                client_order_id: None,
                source: athenas_pallas::events::OrderIntentSource::User,
                strategy_id: None,
            });
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let instrument = InstrumentId::new("binance", "BTCUSDT");
    let mut instruments = HashMap::new();
    instruments.insert(
        instrument.clone(),
        InstrumentMeta::spot(Asset("BTC".into()), Asset("USDT".into())),
    );

    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    balances.insert(Asset("BTC".into()), Decimal::ZERO);

    let registry = InstrumentRegistry::from_instruments(instruments);
    let state = GlobalState::new(registry, balances);
    let strategy = DemoStrategy {
        instrument: instrument.clone(),
        fired: false,
    };
    let risk = RiskPipeline::new(vec![
        Box::new(PauseCheck::default()),
        Box::new(athenas_pallas::risk::TradingDisabledCheck::default()),
    ]);
    let exec = Arc::new(PaperGateway::new(PaperConfig::default()));

    let (handle, _join, _audit) = EngineBuilder::spawn(
        EngineConfig {
            command_channel_capacity: Some(64),
            ..EngineConfig::default()
        },
        state,
        strategy,
        risk,
        exec,
    );

    let token = std::env::var("PALLAS_CONTROL_TOKEN").unwrap_or_else(|_| "dev".into());
    let ctl_handle = handle.clone();
    let bind = "127.0.0.1:9847".to_string();
    let token_clone = token.clone();
    tokio::spawn(async move {
        if let Err(e) = serve(
            ctl_handle,
            ControlServerConfig {
                bind,
                secret: token_clone,
            },
        )
        .await
        {
            warn!("control server ended: {e}");
        }
    });

    info!(
        "control: POST http://127.0.0.1:9847/{{pause,resume,cancel-all}} header {}: {}",
        HEADER_SECRET,
        token
    );

    let connector = BinanceCombinedStream {
        instrument: instrument.clone(),
        stream_symbol: "btcusdt".into(),
        ws_base: "wss://stream.binance.com:9443".into(),
    };
    let md_handle = handle.clone();
    tokio::spawn(async move {
        if let Err(e) = connector.run(md_handle).await {
            warn!("market connector: {e}");
        }
    });

    info!("streaming Binance public data; Ctrl+C to exit");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
