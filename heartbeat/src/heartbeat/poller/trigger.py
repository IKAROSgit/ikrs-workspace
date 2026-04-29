"""Ad-hoc tick trigger with rate limiting.

Calls `sudo systemctl start ikrs-heartbeat.service` at most once per
10 seconds. State held in a timestamp file so the rate limit survives
restarts (conservative: restart resets to "can trigger immediately").
"""

from __future__ import annotations

import logging
import subprocess
import time
from pathlib import Path

logger = logging.getLogger("heartbeat.poller.trigger")

DEFAULT_TIMESTAMP_PATH = Path("/var/lib/ikrs-heartbeat/last-trigger.timestamp")
MIN_INTERVAL_SECONDS = 10


def _read_last_trigger(path: Path) -> float:
    """Read the last trigger timestamp from file. Returns 0.0 if missing."""
    try:
        return float(path.read_text().strip())
    except (OSError, ValueError):
        return 0.0


def _write_timestamp(path: Path, ts: float) -> None:
    """Write a timestamp to file (as text). More testable than mtime."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(str(ts))


def maybe_trigger_tick(
    path: Path = DEFAULT_TIMESTAMP_PATH,
    *,
    _now: float | None = None,
) -> bool:
    """Trigger an ad-hoc tick if rate limit allows. Returns True if triggered."""
    now = _now or time.time()
    last = _read_last_trigger(path)

    if now - last < MIN_INTERVAL_SECONDS:
        logger.debug(
            "rate-limited: last trigger %.1fs ago, min interval %ds",
            now - last, MIN_INTERVAL_SECONDS,
        )
        return False

    try:
        result = subprocess.run(
            ["sudo", "/usr/bin/systemctl", "start", "ikrs-heartbeat.service"],
            capture_output=True,
            text=True,
            timeout=5,
        )
        if result.returncode != 0:
            logger.warning(
                "systemctl start failed: rc=%d stderr=%s",
                result.returncode, result.stderr.strip(),
            )
            return False
    except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
        logger.warning("systemctl start error: %s", exc)
        return False

    _write_timestamp(path, now)
    logger.info("triggered ad-hoc tick")
    return True
