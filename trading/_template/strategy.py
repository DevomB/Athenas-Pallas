"""No-op strategy starter. Copy this directory and implement on_event."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "_sdk" / "python"))

from pallas_strategy import Ctx, run


def on_event(_ctx: Ctx, _event: dict) -> list[dict]:
    return []


if __name__ == "__main__":
    run(on_event)
