# Simple SMA crossover

SMA 5/20 crossover on bar close. Buys when fast crosses above slow; sells flat on death cross.

## Run

From `Backtesting-Engine`:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv \
  --instrument test:EXAMPLE \
  --initial-balance USD:10000 \
  --strategy simple_sma \
  --output results.json
```

Warmup: no orders until the 20-bar slow window is full.
