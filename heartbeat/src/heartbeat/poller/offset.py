"""Telegram update offset persistence.

Stores the last-processed update_id at a file path (default:
/var/lib/ikrs-heartbeat/telegram-offset). Atomic writes via
mkstemp+fsync+rename so a crash mid-write can't corrupt the file.
"""

from __future__ import annotations

import logging
import os
import tempfile
from pathlib import Path

logger = logging.getLogger("heartbeat.poller.offset")

DEFAULT_OFFSET_PATH = Path("/var/lib/ikrs-heartbeat/telegram-offset")


def read_offset(path: Path = DEFAULT_OFFSET_PATH) -> int | None:
    """Read persisted offset. Returns None if file doesn't exist."""
    if not path.exists():
        return None
    try:
        text = path.read_text().strip()
        result: int = int(text)
        return result
    except (ValueError, OSError) as exc:
        logger.warning("corrupt offset file at %s: %s; treating as first start", path, exc)
        return None


def write_offset(offset: int, path: Path = DEFAULT_OFFSET_PATH) -> None:
    """Persist offset atomically (mkstemp+fsync+rename)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(path.parent), prefix=".offset-")
    try:
        os.write(fd, str(offset).encode("ascii"))
        os.fsync(fd)
        os.close(fd)
        os.rename(tmp, str(path))
    except Exception:
        os.close(fd) if not os.get_inheritable(fd) else None  # noqa: E501
        import contextlib

        with contextlib.suppress(OSError):
            os.unlink(tmp)
        raise
