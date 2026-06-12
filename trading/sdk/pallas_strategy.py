"""Bootstrap for external strategies talking to pallas-backtest."""

from __future__ import annotations

import json
import sys
from collections import deque
from dataclasses import dataclass
from typing import Any, Callable, List, Optional


@dataclass
class Ctx:
    position_qty: str
    mid: Optional[str]
    equity: str
    balances: dict


def _read_line() -> dict:
    line = sys.stdin.readline()
    if not line:
        raise SystemExit(0)
    return json.loads(line)


def _write_intents(seq: int, intents: List[dict]) -> None:
    sys.stdout.write(
        json.dumps({"msg": "intents", "seq": seq, "intents": intents}) + "\n"
    )
    sys.stdout.flush()


def run(on_event: Callable[[Ctx, dict], List[dict]], on_init: Optional[Callable[[dict], None]] = None) -> None:
    msg = _read_line()
    if msg.get("msg") != "init":
        raise RuntimeError(f"expected init, got {msg}")
    if on_init:
        on_init(msg)
    sys.stdout.write(json.dumps({"msg": "ready"}) + "\n")
    sys.stdout.flush()

    while True:
        line = sys.stdin.readline()
        if not line:
            break
        msg = json.loads(line)
        if msg.get("msg") == "shutdown":
            break
        if msg.get("msg") != "event":
            continue
        ctx = Ctx(**msg["ctx"])
        intents = on_event(ctx, msg["event"])
        _write_intents(msg["seq"], intents)


class RollingSma:
    """O(1) rolling simple moving average."""

    def __init__(self, window: int) -> None:
        self.window = window
        self.buf: deque[float] = deque(maxlen=window)
        self.total = 0.0

    def update(self, value: float) -> Optional[float]:
        if len(self.buf) == self.window:
            self.total -= self.buf[0]
        self.buf.append(value)
        self.total += value
        if len(self.buf) < self.window:
            return None
        return self.total / self.window
