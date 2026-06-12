use athenas_pallas::backtest::{default_tick_size, BarSeries, BarSeriesSource, HistoricalSource};

use athenas_pallas::dispatch_event_sync;

use athenas_pallas::execution::{PaperConfig, SyncPaperGateway};

use athenas_pallas::risk::{PauseCheck, RiskPipeline};

use athenas_pallas::state::{GlobalState, InstrumentRegistry};

use athenas_pallas::strategy::NoopStrategy;

use athenas_pallas::types::{Asset, ExchangeId, InstrumentId, Symbol};

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use rust_decimal::Decimal;

use std::collections::HashMap;



fn bench_noop(c: &mut Criterion) {

    c.bench_function("noop_100k_bars", |b| {

        b.iter(|| {

            let inst = InstrumentId::new("binance", "BTCUSDT");

            let mut map = HashMap::new();

            map.insert(

                inst.clone(),

                athenas_pallas::instrument::InstrumentMeta::spot("BTC", "USDT"),

            );

            let mut balances = HashMap::new();

            balances.insert(Asset::new("USDT"), Decimal::new(10_000, 0));

            let mut state = GlobalState::new(InstrumentRegistry::from_instruments(map), balances);

            let mut strategy = NoopStrategy;

            let risk = RiskPipeline::new(vec![Box::new(PauseCheck::default())]);

            let exec = SyncPaperGateway::new(PaperConfig::default());

            let series = BarSeries::random_walk(

                100_000,

                42,

                Decimal::new(40_000, 0),

                default_tick_size(),

            );

            let mut src = BarSeriesSource::new(

                series,

                ExchangeId::new("binance"),

                Symbol::new("BTCUSDT"),

            );

            let mut intents = Vec::new();

            while let Some(ev) = src.next_event() {

                dispatch_event_sync(

                    black_box(&mut state),

                    black_box(&mut strategy),

                    &risk,

                    &exec,

                    ev,

                    &mut intents,

                )

                .unwrap();

            }

        });

    });

}



criterion_group!(benches, bench_noop);

criterion_main!(benches);

