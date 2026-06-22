# Local market data (gitignored)

This folder is your workspace for local market-history files. Nothing here is committed to the repo.

## Add bars

Export or copy a CSV into this directory, then point `pallas-backtest --data` or `[backtest].data` in your TOML at that file.

## Resample offline

Aggregate a finer CSV to a coarser interval (tools crate):

```bash
cargo run --release -p athenas-pallas-tools --bin pallas-resample -- \
  --input data/BTCUSDT_1m.csv --to 30m -o data/BTCUSDT_30m.csv
```

## CSV schemas by asset class

### Crypto / generic OHLCV (`DataFormat::Ohlcv`)

Used by most crypto and generic OHLCV backtests.

| Column  | Type    | Description                          |
|---------|---------|--------------------------------------|
| `ts`    | string  | RFC3339 or `YYYY-MM-DD HH:MM:SS`     |
| `open`  | decimal | Bar open                             |
| `high`  | decimal | Bar high                             |
| `low`   | decimal | Bar low                              |
| `close` | decimal | Bar close                            |
| `volume`| decimal | Base or quote volume (venue-specific)|

Example:

```csv
ts,open,high,low,close,volume
2024-01-01T00:00:00Z,40000,40100,39900,40050,12.5
```

Set `asset_class = "crypto"`. For Sharpe annualization, set `bar_interval = "1h"` or enable `auto_periods_per_year = true` in TOML. Set explicit `base_asset` / `quote_asset` when the symbol does not encode them (e.g. `BTC` / `USDT`).

### Equities (`DataFormat::Ohlcv` or `yahoo`)

Use OHLCV `ts,open,high,low,close,volume` or Yahoo `Date,Open,High,Low,Close,Volume` exports.

Set `asset_class = "equity"`. Optional `session_filter = "equity_rth"` filters to US regular hours if you import intraday bars.

### Forex (`DataFormat::Fx`)

L1 quote snapshots (not OHLCV). Used for spread-aware FX replay.

| Column      | Type    | Description        |
|-------------|---------|--------------------|
| `timestamp` | string  | RFC3339            |
| `bid`       | decimal | Bid                |
| `ask`       | decimal | Ask                |

Set `asset_class = "forex"`. Optional `session_filter = "forex_245"` for Sunday–Friday FX hours.

**Free FX data (manual export):** Dukascopy Historical Data Export, TrueFX (historical tick/quote CSV), or broker exports.

### Bonds (`AssetClass::Bond`)

Bond economics use config metadata; CSV can match OHLCV layout for price history.

| Config field | Example | Role |
|--------------|---------|------|
| `asset_class` | `"bond"` | Enables coupon schedule in replay |
| `coupon_rate` | `0.05` | Set via bond meta defaults in TOML (5% annual) |
| `face_value` | `1000` | Par value per unit |
| `expiry` / maturity | `2030-06-01` | Maturity for option-style expiry hooks |

Coupons are applied on scheduled dates during replay (`backtest/lifecycle.rs`). **Yield/duration reporting is not yet implemented.**

### Futures (`DataFormat::Future`)

Same columns as OHLCV. Contract economics come from config, not the CSV.

Required TOML instrument fields:

- `contract_multiplier` (e.g. `50` for ES)
- `tick_size` (e.g. `0.25`)
- optional `lot_size`, `expiry`

Set `asset_class = "future"`, `data_format = "future"`.

## Backtest config hints

```toml
[instrument]
exchange = "test"
symbol = "EXAMPLE"
asset_class = "equity"
base_asset = "EXAMPLE"
quote_asset = "USD"

[backtest]
data = "data/EXAMPLE_1d.csv"
data_format = "yahoo"
bar_interval = "1d"
auto_periods_per_year = true
session_filter = "none"   # or equity_rth, forex_245

[[balances]]
asset = "USD"
amount = "10000"
```
