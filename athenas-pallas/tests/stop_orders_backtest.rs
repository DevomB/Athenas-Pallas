//! Stop-market order triggers on intrabar high/low.

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::events::{Event, OrderIntent, OrderIntentSource};
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{InstrumentId, OrderType, Side};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

struct StopTestStrategy {
    instrument: InstrumentId,
    placed: bool,
}

impl Strategy for StopTestStrategy {
    fn on_event(&mut self, ctx: &StrategyContext, event: &Event, out: &mut Vec<OrderIntent>) {
        if self.placed {
            return;
        }
        if ctx.state.mid_or_last(&self.instrument).is_none() {
            return;
        }
        if let Event::Market(athenas_pallas::events::MarketEvent::Bar { .. }) = event {
            self.placed = true;
            out.push(OrderIntent {
                instrument: self.instrument.clone(),
                side: Side::Buy,
                order_type: OrderType::StopMarket,
                price: None,
                stop_price: Some(Decimal::from(101u64)),
                qty: Decimal::ONE,
                client_order_id: None,
                source: OrderIntentSource::User,
                strategy_id: None,
            });
        }
    }
}

#[test]
fn stop_market_triggers_when_high_crosses_stop() {
    let dir = std::env::temp_dir().join("pallas_stop_test");
    let _ = std::fs::create_dir_all(&dir);
    let csv = dir.join("bars.csv");
    let mut f = std::fs::File::create(&csv).unwrap();
    writeln!(f, "ts,open,high,low,close,volume").unwrap();
    writeln!(f, "2024-01-01T00:00:00Z,100,100,99,100,1").unwrap();
    writeln!(f, "2024-01-01T01:00:00Z,100,102,100,101,1").unwrap();

    let instrument = InstrumentId::new("binance", "BTCUSDT");
    let mut balances = HashMap::new();
    balances.insert(
        athenas_pallas::types::Asset("USDT".into()),
        Decimal::from(10_000u64),
    );

    let cfg = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Ohlcv,
        instrument: instrument.clone(),
        balances,
        ..BacktestConfig::default()
    };

    let mut strategy = StopTestStrategy {
        instrument,
        placed: false,
    };
    let report = BacktestRunner::run_with_strategy(&cfg, &mut strategy).unwrap();
    assert!(report.fill_count >= 1, "expected stop fill");
}
