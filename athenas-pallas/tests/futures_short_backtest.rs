use athenas_pallas::backtest::{BacktestConfig, BacktestRunner, DataFormat};
use athenas_pallas::events::{Event, OrderIntent, OrderIntentSource};
use athenas_pallas::instrument::AssetClass;
use athenas_pallas::strategy::{Strategy, StrategyContext};
use athenas_pallas::types::{Asset, InstrumentId, OrderType, Side};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::Write;

struct ShortThenCover {
    instrument: InstrumentId,
    bars_seen: usize,
}

impl Strategy for ShortThenCover {
    fn on_event(&mut self, _: &StrategyContext<'_>, event: &Event, out: &mut Vec<OrderIntent>) {
        if !matches!(
            event,
            Event::Market(athenas_pallas::events::MarketEvent::Bar { .. })
        ) {
            return;
        }
        self.bars_seen += 1;
        let side = match self.bars_seen {
            1 => Side::Sell,
            2 => Side::Buy,
            _ => return,
        };
        out.push(OrderIntent {
            instrument: self.instrument.clone(),
            side,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            qty: Decimal::ONE,
            client_order_id: None,
            oco_group: None,
            source: OrderIntentSource::User,
            strategy_id: None,
        });
    }
}

#[test]
fn futures_short_cover_realizes_pnl_and_releases_exposure() {
    let path =
        std::env::temp_dir().join(format!("pallas-futures-short-{}.csv", uuid::Uuid::new_v4()));
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "ts,open,high,low,close,volume").unwrap();
    writeln!(file, "2025-01-01,100,100,100,100,1").unwrap();
    writeln!(file, "2025-01-02,90,90,90,90,1").unwrap();
    writeln!(file, "2025-01-03,80,80,80,80,1").unwrap();
    drop(file);

    let instrument = InstrumentId::new("test", "FUT");
    let cfg = BacktestConfig {
        data_path: path.clone(),
        data_format: DataFormat::Ohlcv,
        instrument: instrument.clone(),
        asset_class: AssetClass::Future,
        base_asset: Some("FUT".into()),
        quote_asset: Some("USD".into()),
        balances: HashMap::from([(Asset::new("USD"), Decimal::from(1_000u64))]),
        fee_bps: Decimal::ZERO,
        slippage_bps: Decimal::ZERO,
        half_spread_bps: Decimal::ZERO,
        contract_multiplier: Some(Decimal::from(10u64)),
        margin_initial_rate: Some(Decimal::new(1, 1)),
        lot_size: Some(Decimal::ONE),
        tick_size: Some(Decimal::ONE),
        ..BacktestConfig::default()
    };
    let mut strategy = ShortThenCover {
        instrument: instrument.clone(),
        bars_seen: 0,
    };

    let report = BacktestRunner::run_with_strategy(&cfg, &mut strategy).unwrap();

    assert_eq!(
        report.pnl.parse::<Decimal>().unwrap(),
        Decimal::from(100u64)
    );
    assert_eq!(report.fill_count, 2);
    assert_eq!(report.fills[0].side, Side::Sell);
    assert_eq!(report.fills[1].side, Side::Buy);
    assert_eq!(
        report
            .final_positions
            .iter()
            .find(|position| position.instrument == instrument)
            .unwrap()
            .qty,
        "0"
    );

    std::fs::remove_file(path).unwrap();
}
