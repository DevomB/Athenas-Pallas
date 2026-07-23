"""Protocol roundtrip tests for pallas_strategy SDK."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path

SDK = Path(__file__).resolve().parent


class PallasStrategyTests(unittest.TestCase):
    def test_init_ready_roundtrip(self) -> None:
        script = '''
import sys
sys.path.insert(0, r"%s")
from pallas_strategy import Ctx, run

seen = {}

def on_init(msg):
    seen["instruments"] = msg.get("instruments")

def on_event(ctx: Ctx, event):
  seen["instrument"] = ctx.instrument
  return []

run(on_event, on_init=on_init)
''' % SDK.as_posix()

        proc = subprocess.run(
            [sys.executable, "-c", script],
            input=json.dumps(
                {
                    "msg": "init",
                    "instruments": [{"exchange": "binance", "symbol": "ETHUSDT"}],
                }
            )
            + "\n"
            + json.dumps({"msg": "shutdown"})
            + "\n",
            text=True,
            capture_output=True,
            check=True,
        )
        lines = [ln for ln in proc.stdout.splitlines() if ln.strip()]
        self.assertEqual(
            json.loads(lines[0]), {"msg": "ready", "capabilities": ["finish"]}
        )

    def test_position_size_pct_equity(self) -> None:
        sys.path.insert(0, str(SDK))
        from pallas_strategy import position_size_pct_equity

        self.assertAlmostEqual(position_size_pct_equity(10_000.0, 100.0, 0.1), 10.0)
        self.assertEqual(position_size_pct_equity(10_000.0, 0.0, 0.1), 0.0)
        self.assertEqual(position_size_pct_equity(10_000.0, 100.0, 0.0), 0.0)

    def test_stable_research_primitives(self) -> None:
        sys.path.insert(0, str(SDK))
        from pallas_strategy import (
            Ema,
            Garch11,
            PageCusum,
            ann_vol_from_returns,
            bar_from_event,
            vol_target_weight,
        )

        bar = bar_from_event(
            {
                "Market": {
                    "Bar": {
                        "open": "100",
                        "high": "102",
                        "low": "99",
                        "close": "101",
                        "volume": "12",
                        "ts": "2026-07-23T12:00:00Z",
                    }
                }
            }
        )
        self.assertIsNotNone(bar)
        self.assertEqual(bar.close, 101.0)
        self.assertEqual(bar.ts, "2026-07-23T12:00:00Z")
        self.assertIsNone(
            bar_from_event(
                {
                    "Market": {
                        "Bar": {
                            "open": "nan",
                            "high": 1,
                            "low": 1,
                            "close": 1,
                        }
                    }
                }
            )
        )

        ema = Ema(3)
        self.assertEqual(ema.update(10.0), 10.0)
        self.assertEqual(ema.update(12.0), 11.0)

        garch = Garch11()
        self.assertAlmostEqual(garch.update(0.1), 0.01)
        self.assertAlmostEqual(garch.update(0.2), 0.011001)
        self.assertIsNotNone(garch.zscore(0.2))

        cusum = PageCusum(drift=0.0, threshold=1.0)
        self.assertEqual(cusum.update(0.6), 0)
        self.assertEqual(cusum.update(0.6), 1)

        volatility = ann_vol_from_returns([-0.01, 0.01], bars_per_year=1.0)
        self.assertAlmostEqual(volatility, 2**0.5 * 0.01)
        self.assertEqual(
            vol_target_weight(
                [-0.01, 0.01],
                target_vol=0.20,
                leverage_cap=1.5,
                bars_per_year=1.0,
            ),
            1.5,
        )

    def test_rolling_sma_rejects_zero_window(self) -> None:
        sys.path.insert(0, str(SDK))
        from pallas_strategy import RollingSma

        with self.assertRaises(ValueError):
            RollingSma(0)


if __name__ == "__main__":
    unittest.main()
