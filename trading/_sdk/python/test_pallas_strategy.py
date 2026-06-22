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
        self.assertEqual(json.loads(lines[0]), {"msg": "ready"})

    def test_position_size_pct_equity(self) -> None:
        sys.path.insert(0, str(SDK))
        from pallas_strategy import position_size_pct_equity

        self.assertAlmostEqual(position_size_pct_equity(10_000.0, 100.0, 0.1), 10.0)
        self.assertEqual(position_size_pct_equity(10_000.0, 0.0, 0.1), 0.0)
        self.assertEqual(position_size_pct_equity(10_000.0, 100.0, 0.0), 0.0)


if __name__ == "__main__":
    unittest.main()
