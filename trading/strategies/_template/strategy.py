"""No-op strategy starter — copy this directory and implement on_event."""

from __future__ import annotations

import sys
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[3]
_SDK = _ROOT.parent / "trading" / "sdk"
if not (_SDK / "pallas_strategy.py").is_file():
    _SDK = Path(__file__).resolve().parents[2] / "sdk"
sys.path.insert(0, str(_SDK))

from pallas_strategy import Ctx, run


def on_event(_ctx: Ctx, _event: dict) -> list[dict]:
    return []


if __name__ == "__main__":
    run(on_event)
