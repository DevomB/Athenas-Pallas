//! Stop-market order triggers on intrabar high/low.

use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::dispatch_replay_sync;
use athenas_pallas::events::{Event, MarketEvent, OrderIntent, OrderIntentSource};
use athenas_pallas::execution::{PaperConfig, PaperExecution};
use athenas_pallas::instrument::{InstrumentMeta, InstrumentRegistry};
use athenas_pallas::risk::RiskEngine;
use athenas_pallas::state::GlobalState;
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, InstrumentId, OrderType, Side};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::Write;
use time::macros::datetime;

struct StopTestStrategy {
    instrument: InstrumentId,
    placed: bool,
}

struct FirstBarMarket {
    instrument: InstrumentId,
    qty: Decimal,
    placed: bool,
}

impl Strategy for FirstBarMarket {
    fn on_event(&mut self, _ctx: &StrategyContext, event: &Event, out: &mut Vec<OrderIntent>) {
        if self.placed
            || !matches!(
                event,
                Event::Market(athenas_pallas::events::MarketEvent::Bar { .. })
            )
        {
            return;
        }
        self.placed = true;
        out.push(OrderIntent {
            instrument: self.instrument.clone(),
            side: Side::Buy,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            qty: self.qty,
            client_order_id: Some(athenas_pallas::types::ClientOrderId("entry-1".into())),
            oco_group: None,
            source: OrderIntentSource::User,
            strategy_id: None,
        });
    }

    fn uses_tick_replay(&self) -> bool {
        true
    }
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
                oco_group: None,
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

    let instrument = InstrumentId::new("test", "BTCUSDT");
    let mut balances = HashMap::new();
    balances.insert(
        athenas_pallas::types::Asset("USDT".into()),
        Decimal::from(10_000u64),
    );

    let cfg = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Ohlcv,
        instrument: instrument.clone(),
        asset_class: athenas_pallas::instrument::AssetClass::Crypto,
        base_asset: Some("BTC".into()),
        quote_asset: Some("USDT".into()),
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

fn two_bar_config(name: &str) -> (BacktestConfig, InstrumentId) {
    let dir = std::env::temp_dir().join("pallas_bar_execution_test");
    let _ = std::fs::create_dir_all(&dir);
    let csv = dir.join(name);
    let mut file = std::fs::File::create(&csv).unwrap();
    writeln!(file, "ts,open,high,low,close,volume").unwrap();
    writeln!(file, "2024-01-01T00:00:00Z,100,200,1,100,1").unwrap();
    writeln!(file, "2024-01-02T00:00:00Z,120,130,110,125,1").unwrap();

    let instrument = InstrumentId::new("test", "ABC");
    let balances = HashMap::from([(
        athenas_pallas::types::Asset("USD".into()),
        Decimal::from(10_000u64),
    )]);
    let config = BacktestConfig {
        data_path: csv,
        data_format: DataFormat::Ohlcv,
        instrument: instrument.clone(),
        asset_class: athenas_pallas::instrument::AssetClass::Equity,
        base_asset: Some("ABC".into()),
        quote_asset: Some("USD".into()),
        balances,
        fee_bps: Decimal::ZERO,
        slippage_bps: Decimal::ZERO,
        half_spread_bps: Decimal::from(100u64),
        lot_size: Some(Decimal::ONE),
        ..BacktestConfig::default()
    };
    (config, instrument)
}

#[test]
fn bar_order_fills_on_next_open_with_configured_spread() {
    let (config, instrument) = two_bar_config("next_open.csv");
    let mut strategy = FirstBarMarket {
        instrument,
        qty: Decimal::ONE,
        placed: false,
    };

    let report = BacktestRunner::run_with_strategy(&config, &mut strategy).unwrap();

    assert_eq!(report.fill_count, 1);
    assert_eq!(
        report.fills[0].price.parse::<Decimal>().unwrap(),
        Decimal::new(1_212, 1)
    );
    assert_eq!(
        report.fills[0].client_order_id.as_ref().unwrap().0,
        "entry-1"
    );
    assert_eq!(report.data.processed_events, 2);
    assert!(report.total_fees.parse::<Decimal>().unwrap().is_zero());
}

#[test]
fn direct_replay_dispatch_also_fills_at_next_open() {
    let instrument = InstrumentId::new("test", "ABC");
    let registry = InstrumentRegistry::from_instruments(HashMap::from([(
        instrument.clone(),
        InstrumentMeta::spot(Asset("ABC".into()), Asset("USD".into())),
    )]));
    let mut state = GlobalState::new(
        registry,
        HashMap::from([(Asset("USD".into()), Decimal::from(10_000u64))]),
    );
    state.synthetic_half_spread_bps = Decimal::from(100u64);
    let risk = RiskEngine::default();
    let execution = PaperExecution::new(PaperConfig {
        fee_bps: Decimal::ZERO,
        market_slippage_bps: Decimal::ZERO,
        ..PaperConfig::default()
    });
    let mut strategy = FirstBarMarket {
        instrument: instrument.clone(),
        qty: Decimal::ONE,
        placed: false,
    };
    let mut intents = Vec::new();

    for (ts, open, high, low, close) in [
        (datetime!(2024-01-01 00:00 UTC), 100, 200, 1, 100),
        (datetime!(2024-01-02 00:00 UTC), 120, 130, 110, 125),
    ] {
        let event = Event::Market(MarketEvent::Bar {
            instrument: instrument.clone(),
            ts,
            open: Decimal::from(open),
            high: Decimal::from(high),
            low: Decimal::from(low),
            close: Decimal::from(close),
            volume: Decimal::ONE,
        });
        if let Event::Market(market) = &event {
            state.apply_market(market);
        }
        dispatch_replay_sync(
            &mut state,
            &mut strategy,
            &risk,
            &execution,
            event,
            &mut intents,
        )
        .unwrap();
    }

    assert_eq!(state.fill_log.len(), 1);
    assert_eq!(state.fill_log[0].price, "121.20");
}

#[test]
fn rejected_bar_order_is_visible_in_report() {
    let (config, instrument) = two_bar_config("rejected.csv");
    let mut strategy = FirstBarMarket {
        instrument,
        qty: Decimal::from(1_000u64),
        placed: false,
    };

    let report = BacktestRunner::run_with_strategy(&config, &mut strategy).unwrap();

    assert_eq!(report.fill_count, 0);
    assert_eq!(report.execution_rejection_count, 1);
    assert_eq!(report.risk_rejection_count, 0);
    assert_eq!(report.rejections.len(), 1);
    assert_eq!(
        report.rejections[0].client_order_id.as_ref().unwrap().0,
        "entry-1"
    );
}

#[test]
fn risk_rejected_bar_order_is_visible_in_report() {
    let (mut config, instrument) = two_bar_config("risk_rejected.csv");
    config.max_position_abs = Some(Decimal::new(5, 1));
    let mut strategy = FirstBarMarket {
        instrument,
        qty: Decimal::ONE,
        placed: false,
    };

    let report = BacktestRunner::run_with_strategy(&config, &mut strategy).unwrap();

    assert_eq!(report.fill_count, 0);
    assert_eq!(report.risk_rejection_count, 1);
    assert_eq!(report.execution_rejection_count, 0);
    assert!(report.pending_orders.is_empty());
}

#[test]
fn built_in_buy_and_hold_uses_whole_share_lot() {
    let (config, _) = two_bar_config("whole_share.csv");

    let report = BacktestRunner::run_buy_and_hold(&config).unwrap();

    assert_eq!(report.fill_count, 1);
    assert_eq!(report.fills[0].qty, "1");
}
