//! Backtest example: replay OHLCV rows + simulated execution + metrics.
//!
//! ```text
//! cargo run -p backtest_csv
//! ```

use athenas_pallas::backtest::{CsvBarSource, HistoricalSource};
use athenas_pallas::dispatch_event;
use athenas_pallas::events::{Event, OrderIntent};
use athenas_pallas::execution::{PaperConfig, SimGateway};
use athenas_pallas::metrics::summarize;
use athenas_pallas::risk::{PauseCheck, RiskPipeline};
use athenas_pallas::state::{GlobalState, InstrumentMeta, InstrumentRegistry};
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, EquityPoint, ExchangeId, InstrumentId, OrderType, Side, Symbol};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use std::collections::HashMap;
use std::path::PathBuf;

/// Buys a small amount on the first bar, then holds.
struct BuyAndHold {
    instrument: InstrumentId,
    done: bool,
}

impl Strategy for BuyAndHold {
    fn on_event(&mut self, ctx: &StrategyContext, _event: &Event, out: &mut Vec<OrderIntent>) {
        if self.done || ctx.state.mid_or_last(&self.instrument).is_none() {
            return;
        }
        self.done = true;
        let qty = Decimal::from_f64(0.01).unwrap_or(Decimal::ZERO);
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instrument = InstrumentId::new("binance", "BTCUSDT");
    let mut instruments = HashMap::new();
    instruments.insert(
        instrument.clone(),
        InstrumentMeta::spot("BTC", "USDT"),
    );
    let mut balances = HashMap::new();
    balances.insert(Asset("USDT".into()), Decimal::new(10_000, 0));
    balances.insert(Asset("BTC".into()), Decimal::ZERO);

    let registry = InstrumentRegistry::from_instruments(instruments);
    let mut state = GlobalState::new(registry, balances);
    let mut strategy = BuyAndHold {
        instrument: instrument.clone(),
        done: false,
    };
    let risk = RiskPipeline::new(vec![Box::new(PauseCheck::default())]);
    let exec = SimGateway::new(PaperConfig::default());

    let csv = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("BTCUSDT_1d.csv");
    let mut src = CsvBarSource::from_path(
        &csv,
        ExchangeId("binance".into()),
        Symbol("BTCUSDT".into()),
    )?;
    let mut curve: Vec<EquityPoint> = Vec::new();
    while let Some(ev) = src.next_event() {
        let ts = match &ev {
            Event::Market(athenas_pallas::events::MarketEvent::Bar { ts, .. }) => *ts,
            Event::Market(athenas_pallas::events::MarketEvent::BookL1 { ts, .. }) => *ts,
            Event::Market(athenas_pallas::events::MarketEvent::Trade { ts, .. }) => *ts,
            Event::Market(athenas_pallas::events::MarketEvent::BookL2Snapshot(s)) => s.ts,
            _ => time::OffsetDateTime::now_utc(),
        };
        dispatch_event(&mut state, &mut strategy, &risk, &exec, ev).await?;
        if let Some(eq) = state.mark_to_market_equity(&instrument) {
            curve.push(EquityPoint {
                ts,
                equity_quote: eq,
            });
        }
    }

    let summary = summarize(curve, 252.0);
    println!("PnL: {}", summary.pnl);
    println!("PnL %: {}", summary.pnl_pct);
    println!("Max drawdown (fraction): {}", summary.max_drawdown);
    println!("Sharpe (scaled): {}", summary.sharpe);
    println!("Sortino (scaled): {}", summary.sortino);
    println!("Per-step returns: {:?}", summary.returns);
    Ok(())
}
