# Local market data (gitignored)

This folder is your workspace for downloaded history. Nothing here is committed to the repo.

## Fetch bars

```bash
cargo run -p athenas-pallas --bin pallas-fetch --features data-fetch -- \
  --provider yahoo --symbol AAPL --interval 1d --days 90 \
  -o data/AAPL_live.csv
```

Or use the **Fetch** tab in `pallas-app` (`pnpm tauri dev`).

Point `pallas-backtest --data` or `[backtest].data` in your TOML at a file in this directory.

## Resample offline

Aggregate a finer CSV to a coarser interval without re-fetching:

```bash
cargo run -p athenas-pallas --bin pallas-resample -- \
  --input data/BTCUSDT_1m.csv --to 30m -o data/BTCUSDT_30m.csv
```

## CSV schemas by asset class

### Crypto / generic OHLCV (`DataFormat::Ohlcv`)

Used by Binance fetch and most crypto backtests.

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

Set `asset_class = "crypto"` (default). For Sharpe annualization, set `bar_interval = "1h"` or enable `auto_periods_per_year = true` in TOML.

### Equities (`DataFormat::Yahoo`)

Yahoo Finance daily/intraday export.

| Column   | Type    | Description        |
|----------|---------|--------------------|
| `Date`   | string  | `YYYY-MM-DD`       |
| `Open`   | decimal | Open               |
| `High`   | decimal | High               |
| `Low`    | decimal | Low                |
| `Close`  | decimal | Close              |
| `Adj Close` | decimal | Optional; used for backtest close when present |
| `Volume` | decimal | Share volume       |

When `Adj Close` is present, the loader uses it for the bar close (split/dividend adjusted).

Set `asset_class = "equity"`, `exchange = "yahoo"`. Optional `session_filter = "equity_rth"` filters to US regular hours when using intraday bars.

### Forex (`DataFormat::Fx`)

L1 quote snapshots (not OHLCV). Used for spread-aware FX replay.

| Column      | Type    | Description        |
|-------------|---------|--------------------|
| `timestamp` | string  | RFC3339            |
| `bid`       | decimal | Bid                |
| `ask`       | decimal | Ask                |

Set `asset_class = "forex"`. Optional `session_filter = "forex_245"` for Sunday–Friday FX hours.

**Free FX data (manual export):** Dukascopy Historical Data Export, TrueFX (historical tick/quote CSV), or broker exports. There is no built-in FX fetch adapter yet — use `pallas-fetch` for Binance/Yahoo only.

### Bonds (`AssetClass::Bond`)

Bond economics use config metadata; CSV can match Yahoo or OHLCV layout for price history.

| Config field | Example | Role |
|--------------|---------|------|
| `asset_class` | `"bond"` | Enables coupon schedule in replay |
| `coupon_rate` | `0.05` | Set via bond meta defaults in TOML (5% annual) |
| `face_value` | `1000` | Par value per unit |
| `expiry` / maturity | `2030-06-01` | Maturity for option-style expiry hooks |

Coupons are applied on scheduled dates during replay (`backtest/lifecycle.rs`). **Yield/duration reporting is not yet implemented.**

### Futures (`DataFormat::Future`)

Same columns as Yahoo or OHLCV. Contract economics come from config, not the CSV.

Required TOML instrument fields:

- `contract_multiplier` (e.g. `50` for ES)
- `tick_size` (e.g. `0.25`)
- optional `lot_size`, `expiry`

Set `asset_class = "future"`, `data_format = "future"`.

## Backtest config hints

```toml
[backtest]
data = "data/BTCUSDT_1h.csv"
data_format = "ohlcv"
bar_interval = "1h"
auto_periods_per_year = true
session_filter = "none"   # or equity_rth, forex_245

[instrument]
exchange = "binance"
symbol = "BTCUSDT"
asset_class = "crypto"
```
