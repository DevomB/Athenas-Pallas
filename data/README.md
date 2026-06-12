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
