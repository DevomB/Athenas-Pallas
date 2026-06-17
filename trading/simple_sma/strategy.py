"""SMA 5/20 crossover on bar close."""

from __future__ import annotations

import sys
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "_sdk" / "python"))

from pallas_strategy import Ctx, RollingSma, run

FAST = 5
SLOW = 20
QTY = "0.01"

fast = RollingSma(FAST)
slow = RollingSma(SLOW)
prev_sign: int | None = None


def bar_close(event: dict) -> float | None:
    market = event.get("Market") or event.get("market")
    if not market:
        return None
    bar = market.get("Bar") or market.get("bar")
    if not bar:
        return None
    return float(bar["close"])


def on_event(ctx: Ctx, event: dict) -> list[dict]:
    global prev_sign
    close = bar_close(event)
    if close is None:
        return []

    f = fast.update(close)
    s = slow.update(close)
    if f is None or s is None:
        return []

    sign = 1 if f > s else -1 if f < s else 0
    intents: list[dict] = []

    if sign != 0 and sign != prev_sign:
        inst = ctx.instrument or {"exchange": "binance", "symbol": "BTCUSDT"}
        if sign > 0:
            intents.append(
                {
                    "instrument": inst,
                    "side": "Buy",
                    "order_type": "Market",
                    "qty": QTY,
                }
            )
        else:
            pos = Decimal(ctx.position_qty)
            if pos > 0:
                intents.append(
                    {
                        "instrument": inst,
                        "side": "Sell",
                        "order_type": "Market",
                        "qty": str(pos),
                    }
                )
    if sign != 0:
        prev_sign = sign
    return intents


if __name__ == "__main__":
    run(on_event)
