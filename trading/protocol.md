# Strategy protocol

Newline-delimited JSON on stdin/stdout between `pallas-backtest` and a strategy process.

## Engine → strategy

**init**
```json
{"msg":"init","instruments":[{"exchange":"binance","symbol":"BTCUSDT","base":"BTC","quote":"USDT"}],"balances":{"USDT":"10000"},"config":{"fee_bps":"10"}}
```

**event** (one per market event)
```json
{"msg":"event","seq":1,"event":{...},"ctx":{"position_qty":"0","mid":"40000","equity":"10000","balances":{"USDT":"10000"}}}
```

**shutdown**
```json
{"msg":"shutdown"}
```

## Strategy → engine

**ready** (once after init)
```json
{"msg":"ready"}
```

**intents** (response to each event; `seq` must match)
```json
{"msg":"intents","seq":1,"intents":[{"instrument":{"exchange":"binance","symbol":"BTCUSDT"},"side":"Buy","order_type":"Market","qty":"0.01"}]}
```

Decimal fields are strings. `side` is `Buy` or `Sell`. `order_type` is `Market` or `Limit`.
