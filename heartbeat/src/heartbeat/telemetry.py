"""``heartbeat_health`` Firestore document writer (E.5 lands the real impl).

E.1 ships only the document shape so the spec stays grounded in code.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from datetime import datetime
from typing import Any


@dataclass(frozen=True)
class HeartbeatHealthDoc:
    """One row per tick. Mirrors spec §Telemetry.

    Both Tier I and Tier II write here. Distinguished by ``tier`` field.
    """

    tenantId: str
    engagementId: str
    tier: str  # "I" | "II"
    tickTs: str  # ISO-8601 with TZ
    status: str  # ok | error | skipped | no-op
    durationMs: int
    tokensUsed: int
    promptVersion: str
    actionsEmitted: int
    errorCode: str | None
    expiresAt: str  # ISO-8601 with TZ; 30-day TTL

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def now_iso() -> str:
    """Local helper — spec stores tickTs in operator-local TZ."""

    return datetime.now().astimezone().isoformat()
