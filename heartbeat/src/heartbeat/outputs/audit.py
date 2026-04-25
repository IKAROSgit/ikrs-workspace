"""Audit log — JSONL append per tick + per emitted action.

Lives at ``<vault_root>/_memory/heartbeat-log.jsonl`` by default. Append-
only by design: every tick writes one ``"kind": "tick"`` line, plus one
``"kind": "action"`` line per emitted action. Operator can grep / tail
the file at any time without coordinating with the heartbeat process.

Atomicity: each line is written via a single ``write()`` of a
newline-terminated JSON document. Linux append() of a buffer ≤ PIPE_BUF
(4096 bytes typical) is atomic with respect to other appenders. We keep
each line under that limit by truncating long fields before serialising.
For longer payloads the worst case is *interleaving*, not corruption —
acceptable for a log file.

This module ALSO de-duplicates action lines by ID: if the same
action.id has already been written this engagement-lifetime, we skip the
duplicate. Prevents a state-save-failure-then-retry cycle from logging
the same action twice (per E.4 post-code challenge concern).
"""

from __future__ import annotations

import json
import logging
import os
from dataclasses import asdict
from pathlib import Path
from typing import Any

from heartbeat.actions import (
    Action,
    KanbanTaskAction,
    MemoryUpdateAction,
    TelegramPushAction,
)

logger = logging.getLogger("heartbeat.outputs.audit")


# Hard cap for any string field we serialise. Keeps each JSON line under
# PIPE_BUF (4096 bytes) so the OS's append-write atomicity guarantee
# holds even when multiple processes happen to share the file.
_MAX_FIELD_BYTES = 800


class AuditError(RuntimeError):
    def __init__(self, message: str, *, error_code: str = "audit_error") -> None:
        super().__init__(message)
        self.error_code = error_code


def append_tick_line(
    *,
    audit_log_path: Path,
    tenant_id: str,
    engagement_id: str,
    tick_ts: str,
    status: str,
    duration_ms: int,
    actions_emitted: int,
    error_code: str | None,
    summary: str,
    collector_errors: list[Any],
    tokens_used: int,
    prompt_version: str,
) -> None:
    """Write the per-tick JSONL line."""

    line = {
        "kind": "tick",
        "tenantId": tenant_id,
        "engagementId": engagement_id,
        "tickTs": tick_ts,
        "status": status,
        "durationMs": duration_ms,
        "actionsEmitted": actions_emitted,
        "errorCode": error_code,
        "summary": _truncate(summary),
        "collectorErrors": [
            {
                "source": e.source,
                "errorCode": e.error_code,
                "message": _truncate(e.message),
            }
            for e in collector_errors
        ],
        "tokensUsed": tokens_used,
        "promptVersion": prompt_version,
    }
    _append_line(audit_log_path, line)


def append_action_line(
    *,
    audit_log_path: Path,
    tenant_id: str,
    engagement_id: str,
    action: Action,
    dispatch_status: str = "ok",
    dispatch_error: str | None = None,
) -> None:
    """Write one per-action JSONL line.

    ``dispatch_status`` records whether downstream dispatch (Firestore /
    Telegram) succeeded. The action itself is already typed and known
    well-formed by this point.
    """

    payload: dict[str, Any] = {
        "kind": "action",
        "tenantId": tenant_id,
        "engagementId": engagement_id,
        "actionId": action.id,
        "type": action.type,
        "emittedAt": action.emitted_at,
        "dispatchStatus": dispatch_status,
        "dispatchError": dispatch_error,
    }
    if isinstance(action, KanbanTaskAction):
        payload["title"] = _truncate(action.title)
        payload["priority"] = action.priority
        payload["description"] = _truncate(action.description)
        payload["rationale"] = _truncate(action.rationale)
    elif isinstance(action, MemoryUpdateAction):
        payload["note"] = _truncate(action.note)
        payload["tags"] = action.tags[:10]  # cap excessive tag spam
    elif isinstance(action, TelegramPushAction):
        payload["message"] = _truncate(action.message)
        payload["urgency"] = action.urgency
    _append_line(audit_log_path, payload)


def is_action_already_logged(audit_log_path: Path, action_id: str) -> bool:
    """Return True if a previous tick logged ``action_id``.

    Linear scan of the JSONL file. For a 100K-line audit log this is
    ~10 ms — fine for hourly cadence. If audit logs grow huge we can
    add a sidecar index, but that's a Phase F concern.
    """

    if not audit_log_path.exists():
        return False
    needle = f'"actionId": "{action_id}"'
    try:
        with audit_log_path.open("r", encoding="utf-8") as fh:
            for line in fh:
                if needle in line:
                    return True
    except OSError as exc:
        logger.warning("audit log scan failed for %s: %s", audit_log_path, exc)
        return False
    return False


def _append_line(path: Path, payload: dict[str, Any]) -> None:
    """Append a single JSON line, ensuring the parent dir exists."""

    path.parent.mkdir(parents=True, exist_ok=True)
    line = json.dumps(payload, sort_keys=True) + "\n"
    if len(line.encode("utf-8")) > 4096:
        # Beyond PIPE_BUF — atomicity not guaranteed. Log the warning;
        # the line still gets written (interleaving with other writers
        # is the worst case, not corruption of our line itself).
        logger.warning(
            "audit line for kind=%s exceeds 4096 bytes (%d) — atomicity "
            "with concurrent appenders not guaranteed",
            payload.get("kind"),
            len(line.encode("utf-8")),
        )
    try:
        # O_APPEND on POSIX is atomic for writes ≤ PIPE_BUF, even across
        # processes. ``a`` mode + os.write keeps that guarantee.
        fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)
        try:
            os.write(fd, line.encode("utf-8"))
        finally:
            os.close(fd)
    except OSError as exc:
        raise AuditError(
            f"failed to append to {path}: {exc}",
            error_code="audit_write_failed",
        ) from exc


def _truncate(value: str) -> str:
    """Truncate a string field to ``_MAX_FIELD_BYTES`` to keep JSONL
    lines under PIPE_BUF (atomicity boundary)."""

    if not value:
        return ""
    encoded = value.encode("utf-8")
    if len(encoded) <= _MAX_FIELD_BYTES:
        return value
    # Slice carefully on a UTF-8 boundary.
    return encoded[: _MAX_FIELD_BYTES].decode("utf-8", errors="ignore") + "…"


def _action_to_dict(action: Action) -> dict[str, Any]:
    return asdict(action)
