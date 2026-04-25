"""Top-level output dispatch.

Wires together Firestore + Telegram + audit log + heartbeat_health.
Called by main.py after run_tick produces a TickResult.

Per E.4 post-code challenge concern #3, dispatch is a no-op when
``result.status == "error"`` — the actions list may be incomplete or
inconsistent with state, so we don't risk emitting half-baked output.
The audit log + heartbeat_health doc still get written so operators
can see the failure.

Per concern #5, action-ID dedupe is checked against the audit log
before dispatching: if a previous tick already logged an action with
the same ID (e.g. state save failed and the tick was retried), we
skip dispatch but DO record a dedupe entry in the audit log so the
operator can reason about why the action seemed to "vanish".
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from heartbeat.actions import (
    KanbanTaskAction,
    MemoryUpdateAction,
    TelegramPushAction,
)
from heartbeat.config import HeartbeatConfig
from heartbeat.outputs.audit import (
    AuditError,
    append_action_line,
    append_tick_line,
    is_action_already_logged,
)
from heartbeat.outputs.firestore import (
    FirestoreError,
    get_firestore_client,
    write_heartbeat_health,
    write_kanban_task,
)
from heartbeat.outputs.secrets import OutputSecrets
from heartbeat.outputs.telegram import TelegramError, send_telegram_push
from heartbeat.telemetry import HeartbeatHealthDoc, Tier
from heartbeat.tick import TickResult

logger = logging.getLogger("heartbeat.outputs.dispatch")


@dataclass(frozen=True)
class DispatchResult:
    """What dispatch actually managed to do."""

    actions_dispatched: int
    actions_skipped_dedupe: int
    actions_failed: int
    telemetry_written: bool
    audit_lines_written: int


def dispatch_outputs(
    config: HeartbeatConfig,
    secrets: OutputSecrets,
    result: TickResult,
    *,
    now: datetime | None = None,
    tier: Tier = "II",
) -> DispatchResult:
    """Route a TickResult to all configured output sinks.

    Behaviour is best-effort: every sink failure is recorded but never
    raised. Returning a ``DispatchResult`` so the caller (main.py) can
    log a single summary line.
    """

    now = now or datetime.now().astimezone()
    audit_path = config.audit_log_path
    actions_dispatched = 0
    actions_skipped = 0
    actions_failed = 0
    audit_lines = 0

    # ---- Per-action dispatch (skipped if tick errored) ---------------

    if result.status == "error":
        logger.warning(
            "tick status=error error_code=%s — skipping action dispatch "
            "to avoid emitting half-baked output. Actions: %d.",
            result.error_code,
            len(result.actions),
        )
    else:
        for action in result.actions:
            # Dedupe by action.id against the local audit log. Cheap
            # protection against state-save-failure-then-retry duplicates.
            if is_action_already_logged(audit_path, action.id):
                logger.info(
                    "skipping already-logged action %s (dedupe)", action.id
                )
                _try_audit(
                    audit_path,
                    config.tenant_id,
                    config.engagement_id,
                    action,
                    "deduped",
                    None,
                )
                actions_skipped += 1
                audit_lines += 1
                continue

            dispatch_status = "ok"
            dispatch_error: str | None = None
            try:
                if isinstance(action, KanbanTaskAction):
                    if config.outputs.firestore_enabled:
                        client = get_firestore_client(secrets)
                        write_kanban_task(
                            tenant_id=config.tenant_id,
                            engagement_id=config.engagement_id,
                            action=action,
                            client=client,
                        )
                    else:
                        dispatch_status = "skipped"
                        dispatch_error = "firestore disabled"
                elif isinstance(action, TelegramPushAction):
                    if config.outputs.telegram_enabled:
                        send_telegram_push(secrets=secrets, action=action)
                    else:
                        dispatch_status = "skipped"
                        dispatch_error = "telegram disabled"
                elif isinstance(action, MemoryUpdateAction):
                    # Memory updates are pure audit-log appends.
                    pass
            except (FirestoreError, TelegramError) as exc:
                dispatch_status = "error"
                dispatch_error = f"{exc.error_code}: {exc}"
                logger.warning("dispatch failed for action %s: %s", action.id, exc)

            if dispatch_status == "ok":
                actions_dispatched += 1
            elif dispatch_status == "error":
                actions_failed += 1
            # "skipped" is counted as neither dispatched nor failed.

            if config.outputs.audit_enabled:
                _try_audit(
                    audit_path,
                    config.tenant_id,
                    config.engagement_id,
                    action,
                    dispatch_status,
                    dispatch_error,
                )
                audit_lines += 1

    # Use the tick's own timestamp (when run_tick fired) — not the
    # dispatch clock — so a retried dispatch overwrites the same
    # heartbeat_health doc instead of appending a duplicate.
    # Per E.5 post-code challenge concern.
    tick_ts_for_telemetry = result.tick_ts or now.isoformat()

    # ---- Tick-level audit line ---------------------------------------

    if config.outputs.audit_enabled:
        try:
            append_tick_line(
                audit_log_path=audit_path,
                tenant_id=config.tenant_id,
                engagement_id=config.engagement_id,
                tick_ts=tick_ts_for_telemetry,
                status=result.status,
                duration_ms=result.duration_ms,
                actions_emitted=result.actions_emitted,
                error_code=result.error_code,
                summary=result.summary,
                collector_errors=list(result.collector_errors),
                tokens_used=result.tokens_used,
                prompt_version=result.prompt_version,
            )
            audit_lines += 1
        except AuditError as exc:
            logger.warning("tick audit append failed: %s", exc)

    # ---- Telemetry doc to Firestore ----------------------------------

    telemetry_written = False
    if config.outputs.firestore_enabled:
        tick_id = _make_tick_id_from_ts(
            config.tenant_id, config.engagement_id, tick_ts_for_telemetry
        )
        # 30-day TTL per spec — relative to tickTs.
        from datetime import timedelta

        expires_at = (now + timedelta(days=30)).isoformat()
        doc = HeartbeatHealthDoc(
            tenantId=config.tenant_id,
            engagementId=config.engagement_id,
            tier=tier,
            tickTs=tick_ts_for_telemetry,
            status=result.status,
            durationMs=result.duration_ms,
            tokensUsed=result.tokens_used,
            promptVersion=result.prompt_version,
            actionsEmitted=result.actions_emitted,
            errorCode=result.error_code,
            expiresAt=expires_at,
        )
        try:
            client = get_firestore_client(secrets)
            write_heartbeat_health(doc=doc, tick_id=tick_id, client=client)
            telemetry_written = True
        except FirestoreError as exc:
            logger.warning("heartbeat_health write failed: %s", exc)

    return DispatchResult(
        actions_dispatched=actions_dispatched,
        actions_skipped_dedupe=actions_skipped,
        actions_failed=actions_failed,
        telemetry_written=telemetry_written,
        audit_lines_written=audit_lines,
    )


def _try_audit(
    audit_path: Path,
    tenant_id: str,
    engagement_id: str,
    action: object,
    dispatch_status: str,
    dispatch_error: str | None,
) -> None:
    """Append an action audit line; swallow audit failures (the tick
    already happened — the audit log is a best-effort sidecar)."""
    try:
        # Type narrowing for the audit module.
        from heartbeat.actions import (
            KanbanTaskAction as KT,
        )
        from heartbeat.actions import MemoryUpdateAction as MU
        from heartbeat.actions import TelegramPushAction as TP

        if isinstance(action, (KT, MU, TP)):
            append_action_line(
                audit_log_path=audit_path,
                tenant_id=tenant_id,
                engagement_id=engagement_id,
                action=action,
                dispatch_status=dispatch_status,
                dispatch_error=dispatch_error,
            )
    except AuditError as exc:
        logger.warning("action audit append failed: %s", exc)


def _make_tick_id(tenant_id: str, engagement_id: str, now: datetime) -> str:
    """Deterministic tick ID — same tickTs → same doc, so a retry
    overwrites instead of accumulating.

    Kept for API compatibility with callers (mostly tests). Production
    code should use ``_make_tick_id_from_ts`` with ``result.tick_ts``
    so the tick clock — not the dispatch clock — drives the ID.
    """

    return _make_tick_id_from_ts(tenant_id, engagement_id, now.isoformat())


def _make_tick_id_from_ts(tenant_id: str, engagement_id: str, ts: str) -> str:
    """Deterministic tick ID from a pre-formatted timestamp.

    ``ts`` should be the ISO-8601 from ``TickResult.tick_ts``. Replace
    ``:`` so the ID is firestore-doc-id-safe (Firestore allows colons
    but tools like the console URL-encode them).
    """

    safe_ts = ts.replace(":", "-")
    return f"{tenant_id}__{engagement_id}__{safe_ts}"
