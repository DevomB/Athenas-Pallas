//! Paper trading with mock execution (barter flagship-style, offline iterator feed).

use athenas_pallas::execution::{PaperConfig, PaperGateway, SimGateway};
use athenas_pallas::{
    AuditMode, DefaultRiskManager, EngineFeedMode, IndexedInstruments, LiveClock, SummaryPeriod,
    SystemArgs, SystemBuilder, SystemConfig, TradingState,
};
use athenas_pallas::state::GlobalState;
use rust_decimal::Decimal;
use std::fs;
use std::sync::Arc;

struct Noop;
impl athenas_pallas::strategy::Strategy for Noop {
    fn on_event(
        &mut self,
        _: &athenas_pallas::strategy::StrategyContext<'_>,
        _: &athenas_pallas::events::Event,
    ) -> Vec<athenas_pallas::events::OrderIntent> {
        vec![]
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../system_config/system_config.json");
    let raw = fs::read_to_string(path)?;
    let cfg: SystemConfig = serde_json::from_str(&raw)?;
    let indexed = IndexedInstruments::new(cfg.instruments.clone());
    let registry = indexed.registry().clone();
    let state = GlobalState::new(registry, std::collections::HashMap::new());
    let paper = Arc::new(PaperGateway::new(PaperConfig::default()));
    let _sim = SimGateway::new(PaperConfig::default());
    let mut system = SystemBuilder::new()
        .engine_feed_mode(EngineFeedMode::Iterator)
        .audit_mode(AuditMode::Disabled)
        .trading_state(TradingState::Disabled)
        .build(SystemArgs::new(
            &indexed,
            cfg.clone(),
            Arc::new(LiveClock),
            Noop,
            Arc::new(DefaultRiskManager::default()),
            paper,
            state,
        ))?;
    system.set_trading_state(TradingState::Enabled);
    system.run_iterator(vec![]).await?;
    let (summary, _) = system.shutdown().await?;
    summary
        .trading_summary_generator(Decimal::new(5, 2))
        .print_summary(SummaryPeriod::Daily);
    Ok(())
}
