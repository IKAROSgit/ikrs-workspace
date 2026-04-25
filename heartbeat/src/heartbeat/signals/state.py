"""Last-tick state with atomic writes + schema versioning.

Lives at ``<vault_root>/_memory/heartbeat-state.json`` by default.

Atomic writes (per pre-code research):
- ``tempfile.mkstemp(dir=parent)`` so the rename is on the same fs.
- ``f.flush()`` + ``os.fsync(fd)`` for durability.
- ``os.replace(tmp, path)`` is atomic on POSIX.
- Cleanup the tmp file on any exception so we don't leave detritus.

Schema versioning:
- Append-only dispatch dict ``{from_version: upgrade(dict) -> dict}``.
- ``load_state`` applies migrations in a loop until ``schema_version ==
  CURRENT_STATE_SCHEMA``. Never edit a shipped migration.
"""

from __future__ import annotations

import contextlib
import json
import logging
import os
import tempfile
from collections.abc import Callable
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger("heartbeat.signals.state")


CURRENT_STATE_SCHEMA = 1


@dataclass(frozen=True)
class TickState:
    """What we remember between ticks.

    Kept intentionally small — anything that grows unboundedly per tick
    (every event we've ever seen) belongs in Firestore, not here.

    Field semantics:
    - ``last_tick_ts`` — when the previous tick fired. ``None`` on first run.
    - ``last_seen_event_ids`` — calendar event IDs we've already processed,
      so duplicate notifications don't fire. Pruned to events still inside
      the lookback window.
    - ``last_seen_thread_ids`` — Gmail thread IDs ditto.
    - ``last_action_ids`` — IDs of actions Tier II emitted last tick;
      Tier I (E.7) reads these to verify the action landed.
    - ``last_vault_mtimes`` — vault path → ISO-8601 mtime, used to compute
      the changed-file list each tick.
    """

    schema_version: int = CURRENT_STATE_SCHEMA
    last_tick_ts: str | None = None
    last_seen_event_ids: list[str] = field(default_factory=list)
    last_seen_thread_ids: list[str] = field(default_factory=list)
    last_action_ids: list[str] = field(default_factory=list)
    last_vault_mtimes: dict[str, str] = field(default_factory=dict)


# Append-only. Each entry takes a dict at version N and returns a dict at
# version N+1, with ``schema_version`` bumped. Never edit a shipped entry.
_MIGRATIONS: dict[int, Callable[[dict[str, Any]], dict[str, Any]]] = {
    # 0 → 1: bootstrap from a hypothetical pre-versioned state. Currently
    # unused (E.3 ships v1 from day one), but the slot exists so a future
    # operator who copy-pastes a JSON-without-schema_version file gets a
    # graceful upgrade rather than a crash.
    0: lambda d: {**d, "schema_version": 1},
}


def load_state(path: Path) -> TickState:
    """Read state file, applying migrations to reach ``CURRENT_STATE_SCHEMA``.

    Returns a default ``TickState()`` if the file does not exist (first
    run). Raises ``RuntimeError`` if the file is corrupt or there is no
    migration path — never silently overwrites unknown future-schema data.
    """

    if not path.exists():
        logger.info("state file %s missing; starting fresh", path)
        return TickState()

    try:
        raw: dict[str, Any] = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"corrupt state file at {path}: {exc}. "
            f"Operator: back up the file then delete it to reset state."
        ) from exc

    version = int(raw.get("schema_version", 0))

    while version < CURRENT_STATE_SCHEMA:
        upgrade = _MIGRATIONS.get(version)
        if upgrade is None:
            raise RuntimeError(
                f"no migration from schema_version={version} to "
                f"{CURRENT_STATE_SCHEMA} for state file {path}"
            )
        raw = upgrade(raw)
        version = int(raw["schema_version"])

    if version > CURRENT_STATE_SCHEMA:
        raise RuntimeError(
            f"state file {path} has schema_version={version} > current "
            f"{CURRENT_STATE_SCHEMA}. Refusing to downgrade — please update "
            f"the heartbeat package."
        )

    # Drop unknown keys so a future-version writer doesn't silently leak
    # fields back into the dataclass constructor.
    known = {f.name for f in TickState.__dataclass_fields__.values()}
    filtered = {k: v for k, v in raw.items() if k in known}
    return TickState(**filtered)


def save_state(path: Path, state: TickState) -> None:
    """Atomic write of ``state`` to ``path`` (creates parent dirs)."""

    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(asdict(state), indent=2, sort_keys=True)
    write_atomic(path, payload)


def write_atomic(path: Path, content: str) -> None:
    """Atomic file write on POSIX. Same-filesystem tmp + ``os.replace``.

    Why this shape (per pre-code research):
    - ``tempfile.mkstemp(dir=parent)`` keeps the tmp on the same fs so
      ``os.replace`` is atomic.
    - ``f.flush()`` + ``os.fsync(fd)`` ensures content is on disk before
      the rename — important for ext4's default ``data=ordered`` mode to
      give us durable rename-after-fsync.
    - On any exception, unlink the tmp so we never leave detritus.

    Linux-only; we run on systemd-managed VMs.
    """

    parent = path.parent
    fd, tmp = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=parent)
    tmp_path = Path(tmp)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(content)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp_path, path)
    except BaseException:
        with contextlib.suppress(FileNotFoundError):
            tmp_path.unlink()
        raise
