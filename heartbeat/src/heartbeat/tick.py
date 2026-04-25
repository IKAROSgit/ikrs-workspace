"""Tick orchestrator (E.4 lands the real implementation).

E.1 ships only the import surface so other modules can reference
``run_tick()`` without import-time errors.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass

from heartbeat.config import HeartbeatConfig

logger = logging.getLogger("heartbeat.tick")


@dataclass(frozen=True)
class TickResult:
    status: str  # ok | error | skipped | no-op
    duration_ms: int
    actions_emitted: int
    error_code: str | None = None


def run_tick(config: HeartbeatConfig) -> TickResult:  # pragma: no cover - E.4
    """Run one tick. Implemented in E.4."""

    raise NotImplementedError(
        "run_tick lands in E.4. Use main.py --dry-run to validate config in E.1."
    )
