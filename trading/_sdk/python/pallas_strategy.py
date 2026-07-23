"""Bootstrap for external strategies talking to pallas-backtest."""

from __future__ import annotations

import json
import math
import sys
from collections import deque
from dataclasses import dataclass, field
from typing import Any, Callable, List, Optional, Sequence, Union


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


@dataclass(frozen=True)
class Bar:
    """Validated OHLCV view of a Market.Bar protocol event."""

    open: float
    high: float
    low: float
    close: float
    volume: float
    ts: Optional[str] = None


def bar_from_event(event: dict) -> Optional[Bar]:
    """Return a finite, positive-price bar or None for another event/invalid input."""
    market = event.get("Market") or event.get("market")
    raw = (market.get("Bar") or market.get("bar")) if market else None
    if not raw:
        return None
    try:
        values = [
            float(raw[key])
            for key in ("open", "high", "low", "close")
        ]
        volume = float(raw.get("volume", 0.0))
    except (KeyError, TypeError, ValueError):
        return None
    if not all(math.isfinite(value) for value in (*values, volume)):
        return None
    if min(values) <= 0.0 or volume < 0.0:
        return None
    ts = raw.get("ts") or raw.get("timestamp") or raw.get("time")
    return Bar(*values, volume, str(ts) if ts is not None else None)


def log_return(previous_close: float, close: float) -> Optional[float]:
    """Natural-log return for two positive finite prices."""
    if (
        not math.isfinite(previous_close)
        or not math.isfinite(close)
        or previous_close <= 0.0
        or close <= 0.0
    ):
        return None
    value = math.log(close / previous_close)
    return value if math.isfinite(value) else None


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
        if window <= 0:
            raise ValueError("window must be positive")
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


class Ema:
    """Exponentially weighted moving average initialized from the first value."""

    def __init__(self, span: int) -> None:
        if span <= 0:
            raise ValueError("span must be positive")
        self.alpha = 2.0 / (span + 1.0)
        self.value: Optional[float] = None

    def update(self, value: float) -> float:
        self.value = (
            value
            if self.value is None
            else self.alpha * value + (1.0 - self.alpha) * self.value
        )
        return self.value


@dataclass
class Garch11:
    """Small online GARCH(1,1) conditional-variance estimator."""

    omega: float = 1e-6
    alpha: float = 0.05
    beta: float = 0.90
    variance: Optional[float] = None

    def __post_init__(self) -> None:
        if self.omega < 0.0 or self.alpha < 0.0 or self.beta < 0.0:
            raise ValueError("GARCH coefficients must be nonnegative")
        if self.alpha + self.beta >= 1.0:
            raise ValueError("GARCH alpha + beta must be less than 1")

    def update(self, demeaned_return: float) -> float:
        squared = demeaned_return * demeaned_return
        self.variance = (
            squared
            if self.variance is None
            else max(
                self.omega + self.alpha * squared + self.beta * self.variance,
                1e-12,
            )
        )
        return self.variance

    def zscore(self, demeaned_return: float) -> Optional[float]:
        if self.variance is None or self.variance <= 1e-12:
            return None
        return demeaned_return / math.sqrt(self.variance)


class PageCusum:
    """Two-sided Page CUSUM returning +1/-1 when its threshold is crossed."""

    def __init__(self, drift: float = 0.5, threshold: float = 4.0) -> None:
        if drift < 0.0 or threshold <= 0.0:
            raise ValueError("CUSUM drift must be nonnegative and threshold positive")
        self.drift = drift
        self.threshold = threshold
        self.positive = 0.0
        self.negative = 0.0

    def update(self, value: float) -> int:
        self.positive = max(0.0, self.positive + value - self.drift)
        self.negative = max(0.0, self.negative - value - self.drift)
        alarm = (
            1
            if self.positive > self.threshold
            else -1
            if self.negative > self.threshold
            else 0
        )
        if alarm:
            self.reset()
        return alarm

    def reset(self) -> None:
        self.positive = 0.0
        self.negative = 0.0


def ann_vol_from_returns(
    returns: Sequence[float], bars_per_year: float = 252.0
) -> float:
    """Sample volatility annualized by an explicit bar frequency."""
    if not math.isfinite(bars_per_year) or bars_per_year <= 0.0:
        raise ValueError("bars_per_year must be positive and finite")
    if len(returns) < 2:
        return 0.0
    if not all(math.isfinite(value) for value in returns):
        raise ValueError("returns must be finite")
    mean = sum(returns) / len(returns)
    variance = sum((value - mean) ** 2 for value in returns) / (len(returns) - 1)
    return math.sqrt(max(0.0, variance) * bars_per_year)


def vol_target_weight(
    returns: Sequence[float],
    target_vol: float = 0.10,
    leverage_cap: float = 1.5,
    bars_per_year: float = 252.0,
) -> float:
    """Nonnegative volatility-target weight capped at `leverage_cap`."""
    if target_vol < 0.0 or leverage_cap < 0.0:
        raise ValueError("target_vol and leverage_cap must be nonnegative")
    volatility = ann_vol_from_returns(returns, bars_per_year)
    return 0.0 if volatility <= 1e-12 else min(leverage_cap, target_vol / volatility)


def position_size_pct_equity(equity: float, mid: float, pct: float = 0.1) -> float:
    """Return base qty for `pct` of mark-to-market equity at `mid` (spot-style)."""
    if mid <= 0:
        return 0.0
    return max(0.0, (equity * pct) / mid)
