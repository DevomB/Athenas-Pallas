"""Bootstrap for external strategies talking to pallas-backtest."""

from __future__ import annotations

import json
import sys
from collections import deque
from dataclasses import dataclass, field
from typing import Any, Callable, List, Optional, Union


@dataclass
class Ctx:
    position_qty: str
    mid: Optional[str]
    equity: str
    balances: dict
    instruments: list[dict] = field(default_factory=list)
    pending_orders: list[dict] = field(default_factory=list)
    fills: list[dict] = field(default_factory=list)
    rejections: list[dict] = field(default_factory=list)
    instrument: Optional[dict] = field(default=None)


def _read_line() -> dict:
    line = sys.stdin.readline()
    if not line:
        raise SystemExit(0)
    return json.loads(line)


def _write_intents(seq: int, intents: List[dict], actions: Optional[dict] = None) -> None:
    response = {"msg": "intents", "seq": seq, "intents": intents}
    if actions:
        response.update(actions)
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


def _split_result(result: Union[List[dict], dict]) -> tuple[List[dict], dict]:
    if isinstance(result, dict):
        actions = dict(result)
        return actions.pop("intents", []), actions
    return result, {}


def run(
    on_event: Callable[[Ctx, dict], Union[List[dict], dict]],
    on_init: Optional[Callable[[dict], None]] = None,
    on_finish: Optional[Callable[[Ctx], dict]] = None,
) -> None:
    msg = _read_line()
    if msg.get("msg") != "init":
        raise RuntimeError(f"expected init, got {msg}")
    instruments = msg.get("instruments") or []
    session_instrument: Optional[dict] = instruments[0] if instruments else None
    if on_init:
        on_init(msg)
    sys.stdout.write(json.dumps({"msg": "ready", "capabilities": ["finish"]}) + "\n")
    sys.stdout.flush()

    while True:
        line = sys.stdin.readline()
        if not line:
            break
        msg = json.loads(line)
        if msg.get("msg") == "shutdown":
            break
        if msg.get("msg") == "finish":
            ctx = Ctx(**msg["ctx"], instrument=session_instrument)
            intents, actions = _split_result(on_finish(ctx) if on_finish else {})
            _write_intents(msg["seq"], intents, actions)
            continue
        if msg.get("msg") != "event":
            continue
        ctx = Ctx(**msg["ctx"], instrument=session_instrument)
        intents, actions = _split_result(on_event(ctx, msg["event"]))
        _write_intents(msg["seq"], intents, actions)


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


def position_size_pct_equity(equity: float, mid: float, pct: float = 0.1) -> float:
    """Return base qty for `pct` of mark-to-market equity at `mid` (spot-style)."""
    if mid <= 0:
        return 0.0
    return max(0.0, (equity * pct) / mid)
