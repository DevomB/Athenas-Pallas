# Strategy protocol

Newline-delimited JSON on stdin/stdout between `pallas-backtest` and a strategy process.

## Engine -> strategy

**init**
```json
{
  "msg": "init",
  "protocol_version": 2,
  "instruments": [{
    "exchange": "binance",
    "symbol": "BTCUSDT",
    "base": "BTC",
    "quote": "USDT",
    "asset_class": "crypto",
    "lot_size": "0.001",
    "tick_size": "0.01",
    "contract_multiplier": null,
    "expiry": null,
    "margin_initial_rate": null
  }],
  "balances": {"USDT": "10000"},
  "config": {"fee_bps": "10"},
  "parameters": {"fast_window": 5, "slow_window": 20}
}
```

`instruments` contains the primary instrument and every configured extra instrument. `parameters`
is the arbitrary JSON-compatible `[strategy_parameters]` TOML table (or repeated CLI
`--param KEY=JSON` values).

**event** (one per market event)
```json
{
  "msg": "event",
  "seq": 1,
  "event": {
    "Market": {
      "Bar": {
        "instrument": {"exchange": "binance", "symbol": "BTCUSDT"},
        "ts": "2025-01-02T14:30:00Z",
        "open": "40000",
        "high": "40100",
        "low": "39900",
        "close": "40050",
        "volume": "125"
      }
    }
  },
  "ctx": {
    "position_qty": "0",
    "mid": "40000",
    "equity": "10000",
    "balances": {"USDT": "10000"},
    "instruments": [{"instrument": {"exchange":"binance","symbol":"BTCUSDT"}, "position_qty":"0", "mid":"40000"}],
    "pending_orders": [],
    "fills": [],
    "rejections": []
  }
}
```

`fills` and `rejections` contain updates since the preceding callback. `pending_orders` is the
current working-order snapshot, including engine order ids, client ids, and OCO groups.
Every market/timer event timestamp is an RFC 3339 string normalized to UTC (`Z`). Exchange session
logic must use that instant with an explicit venue calendar; the engine never treats a naive local
timestamp as New York time.

**finish** (only when `ready.capabilities` contains `finish`)
```json
{"msg":"finish","seq":91,"ctx":{...}}
```

The strategy must answer with an `intents` response. Returning `"flatten": true` cancels all
working orders and market-closes every final position before the report is built.

**shutdown**
```json
{"msg":"shutdown"}
```

## Strategy -> engine

**ready** (once after init)
```json
{"msg":"ready","capabilities":["finish"]}
```

Omit the `finish` capability when the strategy does not need a final callback.

**intents** (response to each event; `seq` must match)
```json
{
  "msg": "intents",
  "seq": 1,
  "intents": [{
    "instrument": {"exchange": "binance", "symbol": "BTCUSDT"},
    "side": "Buy",
    "order_type": "StopMarket",
    "qty": "0.01",
    "stop_price": "39000",
    "strategy_id": "sleeve_a",
    "client_order_id": "order-1",
    "oco_group": "bracket-42"
  }],
  "cancel_order_ids": [],
  "cancel_client_order_ids": ["old-order"],
  "cancel_all": false,
  "flatten": false
}
```

Decimal fields are strings. `side` is `Buy` or `Sell`. `order_type` is `Market`, `Limit`, `StopMarket`, or `StopLimit`. Limit/stop-limit orders may include `price`; stop orders include `stop_price`.

Orders sharing a non-null `oco_group` are one-cancels-other siblings: the first fill cancels the
remaining working siblings. Cancellation may target an engine UUID in `cancel_order_ids` or a
strategy-owned id in `cancel_client_order_ids`.

For OHLCV replay, a strategy sees a completed bar before submitting its response. Those orders are
therefore held until the next market update for their instrument; market orders execute from the
next bar open using the configured synthetic half-spread and slippage. Orders left without a future
market update are serialized as `pending_orders` in the final report rather than filled against the
submission bar.
