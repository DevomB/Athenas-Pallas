# Algorithm testing guide

See the [README](../README.md) for install and quickstart. This page is the longer copy-paste flow.

## 1. Add data

Files in `data/` are local only (gitignored). Export or copy your market-history CSVs there before running a backtest.

Optional Databento OHLCV cache/export:

```bash
cargo run --release -p athenas-pallas --features databento --bin pallas-backtest -- \
  --provider databento \
  --dataset EQUS.MINI \
  --symbol AAPL \
  --schema ohlcv-1d \
  --start 01-01-2025 \
  --end 02-01-2025 \
  --instrument databento:AAPL \
  --initial-balance USD:10000 \
  --yes
```

Set `DATABENTO_API_KEY` before fetching uncached data. Add `--estimate-only` to validate the
entitled schema/date range and save a read-only `*.inspect.json` capability/cost result without
downloading market data. The provider path writes a CSV cache under `data/databento/`, then uses
the same backtest replay path as local files. A paid fetch also writes a versioned
`*.manifest.json`; the engine reuses the CSV only when the request fields and SHA-256 still match,
and the backtest report records that manifest path. `--adjustment split-adjusted` and
`--adjustment total-return-adjusted` preserve the raw CSV and write a separate adjusted cache whose
manifest binds the raw data, complete factor response, selected policy, and derived checksum.
For a dated derivative, add `--import-definitions`; inspection includes its cost and the run uses
the persisted point-in-time definition rather than inferring contract economics from the symbol.
Use `--schema trades`, `mbp-1`, `mbp-10`, `status`, or `imbalance` to cache normalized JSONL instead
of OHLCV. Trade and top-of-book files are executable replay inputs; depth/status/auction events are
delivered to strategies with source provenance. No queue-position or market-impact claim is made.
When an MBP-10 snapshot is current, each market order uses displayed-level VWAP and rejects size
beyond that snapshot; this is a per-order depth bound, not persistent queue/depletion simulation.
`--schema statistics` stores official venue statistics separately. Continuous and parent symbols
are rejected until the engine has a configured roll rule and auditable close/open ledger.
Local option replay is limited to explicit European cash settlement and sends that exercise style
in the v2 strategy handshake. Databento option definitions are rejected because their normalized
definition record does not establish exercise style.

## 2. Backtest (built-in buy-and-hold)

Buy-and-hold uses one configured lot by default; pass `--buy-and-hold-qty` (or set
`backtest.buy_and_hold_qty`) when a different quantity is intended. Bar-close signals are eligible
on the next market update, never against the high/low of the submission bar.

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv \
  --instrument test:EXAMPLE \
  --initial-balance USD:10000
```

Equity OHLCV from a local export:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data data/AAPL_live.csv \
  --data-format ohlcv \
  --instrument csv:AAPL \
  --asset-class equity \
  --initial-balance USD:10000
```

## 3. Python strategy

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv \
  --instrument test:EXAMPLE \
  --initial-balance USD:10000 \
  --strategy simple_sma \
  --output target/report.json
```

Crypto-shaped fixture (explicit base/quote):

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv \
  --instrument test:BTCUSDT \
  --asset-class crypto \
  --initial-balance USDT:10000 \
  --strategy simple_sma \
  --output target/report.json
```

## 4. TOML config

```bash
cp backtest.toml.example backtest.toml
# edit [backtest].data to your CSV
cargo run --release -p athenas-pallas --bin pallas-backtest -- --config backtest.toml
```

## 5. Sweep and resample

These live in the separate tools crate:

```bash
cargo build --release -p athenas-pallas-tools

# Parameter grid from TOML (see sweep.toml.example)
cargo run --release -p athenas-pallas-tools --bin pallas-sweep -- \
  --config backtest.toml --sweep sweep.toml.example -o target/sweep.csv

# Strategy catalog (see catalog.toml.example), with bounded parallel workers
cargo run --release -p athenas-pallas-tools --bin pallas-sweep -- \
  --config backtest.toml --catalog catalog.toml.example --jobs 4 -o target/catalog.csv

# Resample bars offline
cargo run --release -p athenas-pallas-tools --bin pallas-resample -- \
  --input data/BTCUSDT_1m.csv --to 30m -o data/BTCUSDT_30m.csv

# Build UTC-aligned bars from a normalized single-instrument trade cache
cargo run --release -p athenas-pallas-tools --bin pallas-resample -- \
  --from trades-jsonl --input data/ESM6_trades.jsonl --to 1m -o data/ESM6_1m.csv
```

Catalog paths are resolved relative to the catalog file. Each `parameters` table is merged over
the base config's `[strategy_parameters]`. The CSV retains one row per strategy in manifest order,
including failures; the command exits nonzero after all rows finish if any backtest failed.

Use `pallas-resample` to materialize a coarser immutable input before sweeping computationally
heavy research estimators. A generic engine feature cache is intentionally not provided: external
strategies do not yet share a stable feature ABI, so the engine cannot safely identify equivalent
features from strategy name and parameters alone.

Trade resampling leaves the event cache untouched and writes `<output>.policy.json` with the
instrument, UTC bucket alignment, OHLC selection, volume rule, and empty-bucket policy. It rejects
mixed-instrument, non-trade, and out-of-order inputs rather than silently constructing ambiguous
bars.

## 6. JSONL event replay

Replay an existing JSONL recording via `read_events_jsonl` and `replay_events_sync` for deterministic strategy debugging without reloading CSV.

## 7. Golden tests (CI)

```bash
cargo test -p athenas-pallas
cargo test -p athenas-pallas --test external_strategy_golden -- --ignored
cargo test -p athenas-pallas --test cpp_strategy_golden -- --ignored
```

Fixtures: `athenas-pallas/tests/fixtures/`.
