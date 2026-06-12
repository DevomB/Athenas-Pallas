use athenas_pallas::backtest::{
    default_tick_size, run_backtest, BarSeries, BarSeriesSource, BacktestConfig, BuyAndHold,
    DataFormat, HistoricalSource,
};
use athenas_pallas::dispatch_event_sync;
use athenas_pallas::dispatch_replay_sync;
use athenas_pallas::execution::{PaperConfig, SyncPaperGateway};
use athenas_pallas::risk::{BacktestChecks, PauseCheck, RiskPipeline};
use athenas_pallas::state::{GlobalState, InstrumentRegistry};
use athenas_pallas::strategy::NoopStrategy;
use athenas_pallas::types::{Asset, ExchangeId, InstrumentId, Symbol};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rust_decimal::Decimal;
use std::collections::HashMap;

fn setup_noop_replay(n_bars: usize) -> (GlobalState, BarSeriesSource, SyncPaperGateway) {
    let inst = InstrumentId::new("binance", "BTCUSDT");
    let mut map = HashMap::new();
    map.insert(
        inst.clone(),
        athenas_pallas::instrument::InstrumentMeta::spot("BTC", "USDT"),
    );
    let mut balances = HashMap::new();
    balances.insert(Asset::new("USDT"), Decimal::new(10_000, 0));
    let state = GlobalState::new(InstrumentRegistry::from_instruments(map), balances);
    let exec = SyncPaperGateway::new(PaperConfig::default());
    let series = BarSeries::random_walk(
        n_bars,
        42,
        Decimal::new(40_000, 0),
        default_tick_size(),
    );
    let src = BarSeriesSource::new(series, ExchangeId::new("binance"), Symbol::new("BTCUSDT"));
    (state, src, exec)
}

fn replay_noop_loop(
    state: &mut GlobalState,
    src: &mut BarSeriesSource,
    exec: &SyncPaperGateway,
    risk: &RiskPipeline,
) {
    let mut strategy = NoopStrategy;
    let mut intents = Vec::with_capacity(4);
    while let Some(ev) = src.next_event() {
        dispatch_event_sync(state, &mut strategy, risk, exec, ev, &mut intents).unwrap();
    }
}

fn bench_noop(c: &mut Criterion) {
    c.bench_function("noop_100k_bars", |b| {
        b.iter(|| {
            let (mut state, mut src, exec) = setup_noop_replay(100_000);
            let risk = RiskPipeline::new(vec![Box::new(PauseCheck::default())]);
            replay_noop_loop(black_box(&mut state), &mut src, &exec, &risk);
        });
    });
}

fn bench_noop_amortized(c: &mut Criterion) {
    let (mut state, mut src, exec) = setup_noop_replay(100_000);
    let checks = BacktestChecks;
    let mut strategy = NoopStrategy;
    let mut intents = Vec::with_capacity(4);
    c.bench_function("noop_100k_amortized", |b| {
        b.iter(|| {
            src.rewind();
            while let Some((bar, ts)) = src.next_bar() {
                state.apply_bar(0, &bar, src.tick_size(), Decimal::from(5u64));
                let ev = src.bar_to_event(&bar, ts);
                dispatch_replay_sync(
                    black_box(&mut state),
                    black_box(&mut strategy),
                    &checks,
                    &exec,
                    ev,
                    &mut intents,
                )
                .unwrap();
            }
        });
    });
}

fn bench_snapshot_cost(c: &mut Criterion) {
    let (state, _, _) = setup_noop_replay(1);
    c.bench_function("snapshot_clone", |b| {
        b.iter(|| black_box(state.snapshot()));
    });
}

fn bench_buy_and_hold(c: &mut Criterion) {
    c.bench_function("buy_and_hold_100k", |b| {
        b.iter(|| {
            let inst = InstrumentId::new("binance", "BTCUSDT");
            let mut map = HashMap::new();
            map.insert(
                inst.clone(),
                athenas_pallas::instrument::InstrumentMeta::spot("BTC", "USDT"),
            );
            let mut balances = HashMap::new();
            balances.insert(Asset::new("USDT"), Decimal::new(10_000, 0));
            let mut state =
                GlobalState::new(InstrumentRegistry::from_instruments(map), balances);
            let mut strategy = BuyAndHold::new(inst.clone(), Decimal::new(1, 2));
            let checks = BacktestChecks;
            let exec = SyncPaperGateway::new(PaperConfig::default());
            let series = BarSeries::random_walk(
                100_000,
                42,
                Decimal::new(40_000, 0),
                default_tick_size(),
            );
            let mut src =
                BarSeriesSource::new(series, ExchangeId::new("binance"), Symbol::new("BTCUSDT"));
            let mut intents = Vec::with_capacity(4);
            while let Some((bar, ts)) = src.next_bar() {
                state.apply_bar(0, &bar, src.tick_size(), Decimal::from(5u64));
                let ev = src.bar_to_event(&bar, ts);
                dispatch_replay_sync(&mut state, &mut strategy, &checks, &exec, ev, &mut intents)
                    .unwrap();
            }
        });
    });
}

fn bench_equity_curve_toggle(c: &mut Criterion) {
    let mut group = c.benchmark_group("equity_curve");
    for record in [true, false] {
        group.bench_with_input(
            BenchmarkId::from_parameter(record),
            &record,
            |b, &record| {
                b.iter(|| {
                    let (mut state, mut src, exec) = setup_noop_replay(10_000);
                    let checks = BacktestChecks;
                    let mut strategy = NoopStrategy;
                    let mut intents = Vec::with_capacity(4);
                    let mut samples = 0usize;
                    while let Some((bar, ts)) = src.next_bar() {
                        state.apply_bar(0, &bar, src.tick_size(), Decimal::from(5u64));
                        let ev = src.bar_to_event(&bar, ts);
                        dispatch_replay_sync(
                            &mut state,
                            &mut strategy,
                            &checks,
                            &exec,
                            ev,
                            &mut intents,
                        )
                        .unwrap();
                        if record {
                            if state.mark_to_market_equity_ix(0).is_some() {
                                samples += 1;
                            }
                        }
                    }
                    black_box(samples);
                });
            },
        );
    }
    group.finish();
}

fn bench_session_overhead(c: &mut Criterion) {
    let tmp = std::env::temp_dir().join("pallas_bench_session.csv");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "ts,open,high,low,close,volume").unwrap();
        for i in 0..100_000 {
            writeln!(
                f,
                "2024-01-{:02} 00:00:00,40000,40100,39900,40000,1",
                (i % 28) + 1
            )
            .unwrap();
        }
    }
    let mut cfg = BacktestConfig::default();
    cfg.data_path = tmp;
    cfg.data_format = DataFormat::Ohlcv;
    cfg.record_equity_curve = false;

    c.bench_function("session_overhead_100k", |b| {
        b.iter(|| {
            run_backtest(black_box(&cfg)).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_noop,
    bench_noop_amortized,
    bench_snapshot_cost,
    bench_buy_and_hold,
    bench_equity_curve_toggle,
    bench_session_overhead
);
criterion_main!(benches);
