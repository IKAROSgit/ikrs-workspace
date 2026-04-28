"""Tick orchestrator — one hour's worth of work.

Pipeline:
1. Load tick state (last_tick_ts, last action IDs, last vault mtimes).
2. Collect signals (calendar + gmail + vault) — never raises; partial
   failures fold into ``bundle.errors``.
3. Render the prompt template with this tick's signals.
4. Call the LLM with structured-output mode → strict JSON.
5. Parse JSON into typed ``Action`` objects.
6. Update tick state (new ts, new mtimes, new action IDs).
7. Save state atomically.
8. Return a ``TickResult`` summarising the run for telemetry.

E.4 produces actions but does NOT dispatch them — that lands in E.5
(Firestore + Telegram + audit log writers). The ``actions`` field on
the result is what E.5 will consume.

Error policy: every step is best-effort. The tick never raises; on a
fatal failure the result has ``status="error"`` and ``error_code``
set to a typed value. Telemetry records exactly one ``errorCode``;
the full per-collector error list goes to the local audit log.
"""

from __future__ import annotations

import json
import logging
import time
import uuid
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING

from heartbeat.actions import (
    TICK_RESPONSE_SCHEMA,
    Action,
    ActionParseError,
    action_summary_line,
    parse_actions,
)
from heartbeat.config import HeartbeatConfig
from heartbeat.llm import LlmClient, LlmError, LlmRequest, make_llm_client
from heartbeat.prompts import (
    CURRENT_PROMPT_VERSION,
    load_prompt_template,
    render_tick_prompt,
)
from heartbeat.signals import collect_signals, load_state, save_state
from heartbeat.signals.base import CollectorError, SignalsBundle
from heartbeat.signals.state import TickState
from heartbeat.telemetry import Status

if TYPE_CHECKING:
    pass

logger = logging.getLogger("heartbeat.tick")


# Cap how many recent action IDs we feed back to the LLM for dedupe.
# Large ID lists waste tokens without improving dedupe quality.
_RECENT_ACTION_ID_WINDOW = 50


# Precedence for collapsing N collector errors into a single telemetry
# error_code. Higher = worse. The orchestrator's own errors override
# any signal-collector error.
_ERROR_PRECEDENCE: dict[str, int] = {
    "oauth_refresh_failed": 90,
    "key_version_unknown": 85,    # Phase F: operator must update encryption key
    "token_decrypt_failed": 85,   # Phase F: encryption key mismatch or corrupt doc
    "missing_token": 80,
    "vault_root_missing": 70,
    "api_call_failed": 60,
    "vault_io_error": 50,
    "network_error": 40,
}


@dataclass(frozen=True)
class TickResult:
    """One tick's outcome — what telemetry + audit log record."""

    status: Status
    duration_ms: int
    actions_emitted: int
    # When this tick FIRED (not when dispatch ran). Used by E.5's
    # ``_make_tick_id`` so a retried dispatch overwrites the same
    # heartbeat_health doc instead of appending a duplicate.
    tick_ts: str = ""
    error_code: str | None = None
    summary: str = ""
    actions: list[Action] = field(default_factory=list)
    tokens_used: int = 0
    prompt_tokens: int = 0
    output_tokens: int = 0
    model_used: str = ""
    prompt_version: str = CURRENT_PROMPT_VERSION
    # Full per-collector errors for the audit log. Telemetry's single
    # ``errorCode`` field is derived from this via ``_pick_error_code``.
    collector_errors: list[CollectorError] = field(default_factory=list)


def run_tick(
    config: HeartbeatConfig,
    *,
    token_path: Path,
    state_path: Path | None = None,
    now: datetime | None = None,
    _client: LlmClient | None = None,
) -> TickResult:
    """Run one tick.

    Test seams (``_client``, ``state_path``, ``now``) let pytest avoid
    real LLM calls and clock drift. Production callers pass only
    ``config`` + ``token_path``.
    """

    started = time.monotonic()
    now = now or datetime.now().astimezone()
    tick_ts_iso = now.isoformat()
    state_path = state_path or _default_state_path(config)
    prompt_version = config.prompt_version or CURRENT_PROMPT_VERSION

    # ---- 1. State ---------------------------------------------------
    try:
        state = load_state(state_path)
    except RuntimeError as exc:
        # Corrupt or unmigrate-able state file — refuse to run rather
        # than silently overwrite.
        return _error_result(
            "state_load_failed",
            started,
            status="error",
            prompt_version=prompt_version,
            message=str(exc),
            tick_ts=tick_ts_iso,
        )

    # ---- 2. Signals -------------------------------------------------
    bundle, new_vault_mtimes = collect_signals(
        config, state, now=now, token_path=token_path
    )

    # ---- 3. Prompt --------------------------------------------------
    try:
        template = load_prompt_template(prompt_version)
    except ValueError as exc:
        return _error_result(
            "prompt_load_failed",
            started,
            status="error",
            prompt_version=prompt_version,
            message=str(exc),
            collector_errors=list(bundle.errors),
            tick_ts=tick_ts_iso,
        )

    # Feed back natural-language summaries (not opaque hex IDs) so the
    # LLM can compare *content* and avoid duplicating itself. Per E.4
    # post-code challenge finding #1.
    rendered = render_tick_prompt(
        template,
        tick_ts=now.isoformat(),
        tenant_id=config.tenant_id,
        engagement_id=config.engagement_id,
        last_tick_ts=state.last_tick_ts or "",
        recent_action_summaries=state.last_action_summaries[-_RECENT_ACTION_ID_WINDOW:],
        calendar_lookahead_hours=config.signals.calendar_lookahead_hours,
        gmail_lookback_hours=config.signals.gmail_lookback_hours,
        bundle=bundle,
    )

    # ---- 4. LLM call ------------------------------------------------
    if _client is None:
        try:
            _client = make_llm_client(config.llm)
        except LlmError as exc:
            return _error_result(
                exc.error_code or "llm_init_failed",
                started,
                status="error",
                prompt_version=prompt_version,
                message=str(exc),
                collector_errors=list(bundle.errors),
                tick_ts=tick_ts_iso,
            )

    request = LlmRequest(
        prompt=rendered,
        system_instruction=None,  # baked into the template body
        response_mime_type="application/json",
        response_json_schema=TICK_RESPONSE_SCHEMA,
    )
    try:
        response = _client.generate(request)
    except LlmError as exc:
        return _error_result(
            exc.error_code or "llm_call_failed",
            started,
            status="error",
            prompt_version=prompt_version,
            message=str(exc),
            collector_errors=list(bundle.errors),
            tick_ts=tick_ts_iso,
        )

    # ---- 5. Parse JSON → typed actions -------------------------------
    try:
        raw_payload = json.loads(response.text)
        summary, actions = parse_actions(raw_payload)
    except (json.JSONDecodeError, ActionParseError) as exc:
        # We still got a real LLM response, so token counts and a status
        # of "error" both belong to telemetry — but we have no actions
        # to emit this tick.
        return _error_result(
            "action_parse_failed",
            started,
            status="error",
            prompt_version=prompt_version,
            message=f"could not parse LLM JSON: {exc}",
            collector_errors=list(bundle.errors),
            tokens_used=response.tokens_used,
            prompt_tokens=response.prompt_tokens,
            output_tokens=response.output_tokens,
            model_used=response.model,
            tick_ts=tick_ts_iso,
        )

    # Re-key action IDs server-side so a stale/repeated LLM ID can't
    # collide with state.last_action_ids. Also stamp emitted_at so
    # E.5's downstream writers (Firestore, audit log) have a stable
    # timestamp independent of the dispatch latency.
    re_keyed: list[Action] = []
    emitted_at = now.isoformat()
    for action in actions:
        re_keyed.append(_rekey_and_stamp(action, uuid.uuid4().hex, emitted_at))

    # ---- 6 + 7. Update state, save atomically ------------------------
    # Persist BOTH IDs (so we can verify dispatch completion next tick)
    # AND human-readable summaries (so the LLM has natural-language
    # dedupe context, not opaque hex). Per E.4 post-code challenge
    # finding #1.
    new_action_summaries = [action_summary_line(a) for a in re_keyed]
    new_state = TickState(
        last_tick_ts=now.isoformat(),
        last_seen_event_ids=_collect_calendar_ids(bundle),
        last_seen_thread_ids=_collect_gmail_ids(bundle),
        last_action_ids=(state.last_action_ids + [a.id for a in re_keyed])[
            -_RECENT_ACTION_ID_WINDOW * 2 :
        ],
        last_action_summaries=(state.last_action_summaries + new_action_summaries)[
            -_RECENT_ACTION_ID_WINDOW :
        ],
        last_vault_mtimes=new_vault_mtimes,
    )
    try:
        save_state(state_path, new_state)
    except OSError as exc:
        # State write failed — telemetry records it but we still return
        # the actions so E.5 can attempt dispatch (this tick's data is
        # not lost).
        logger.warning("state save failed: %s", exc)
        return TickResult(
            status="error",
            duration_ms=_ms_since(started),
            actions_emitted=len(re_keyed),
            tick_ts=now.isoformat(),
            error_code="state_save_failed",
            summary=summary,
            actions=re_keyed,
            tokens_used=response.tokens_used,
            prompt_tokens=response.prompt_tokens,
            output_tokens=response.output_tokens,
            model_used=response.model,
            prompt_version=prompt_version,
            collector_errors=list(bundle.errors),
        )

    # ---- 8. TickResult -----------------------------------------------
    error_code = _pick_error_code(bundle.errors)
    status: Status = "ok"
    if error_code is not None:
        # Partial failure — some signals didn't collect. The tick still
        # produced actions, so call it "ok" for the operator and log
        # the error_code for diagnostics. Spec leaves this nuanced; we
        # err on the side of "the LLM still ran successfully".
        status = "ok"
    if not re_keyed and not error_code:
        status = "no-op"

    return TickResult(
        status=status,
        duration_ms=_ms_since(started),
        actions_emitted=len(re_keyed),
        tick_ts=now.isoformat(),
        error_code=error_code,
        summary=summary,
        actions=re_keyed,
        tokens_used=response.tokens_used,
        prompt_tokens=response.prompt_tokens,
        output_tokens=response.output_tokens,
        model_used=response.model,
        prompt_version=prompt_version,
        collector_errors=list(bundle.errors),
    )


# ---------- helpers --------------------------------------------------


def _default_state_path(config: HeartbeatConfig) -> Path:
    """Default state location: ``<vault_root>/_memory/heartbeat-state.json``.

    Same convention as the audit log so the operator's ``_memory/``
    directory has all the per-engagement state in one place.
    """

    return config.vault_root / "_memory" / "heartbeat-state.json"


def _ms_since(started: float) -> int:
    return int((time.monotonic() - started) * 1000)


def _collect_calendar_ids(bundle: SignalsBundle) -> list[str]:
    if bundle.calendar is None:
        return []
    return [e.id for e in bundle.calendar.upcoming_events if e.id]


def _collect_gmail_ids(bundle: SignalsBundle) -> list[str]:
    if bundle.gmail is None:
        return []
    return [t.id for t in bundle.gmail.threads if t.id]


def _rekey_and_stamp(action: Action, new_id: str, emitted_at: str) -> Action:
    """Return a copy of ``action`` with id + emitted_at set.

    Each ``Action`` dataclass is frozen, so we use ``dataclasses.replace``.
    """

    from dataclasses import replace

    return replace(action, id=new_id, emitted_at=emitted_at)


def _pick_error_code(errors: list[CollectorError]) -> str | None:
    """Reduce N collector errors to one telemetry ``errorCode``.

    Picks the highest-precedence error per ``_ERROR_PRECEDENCE``. Empty
    list → None. Unknown error_code → still considered, ranked at 0.
    """

    if not errors:
        return None
    return max(errors, key=lambda e: _ERROR_PRECEDENCE.get(e.error_code, 0)).error_code


def _error_result(
    error_code: str,
    started: float,
    *,
    status: Status = "error",
    prompt_version: str = CURRENT_PROMPT_VERSION,
    message: str = "",
    collector_errors: list[CollectorError] | None = None,
    tokens_used: int = 0,
    prompt_tokens: int = 0,
    output_tokens: int = 0,
    model_used: str = "",
    tick_ts: str = "",
) -> TickResult:
    if message:
        logger.warning("tick error %s: %s", error_code, message)
    return TickResult(
        status=status,
        duration_ms=_ms_since(started),
        actions_emitted=0,
        tick_ts=tick_ts,
        error_code=error_code,
        summary="",
        actions=[],
        tokens_used=tokens_used,
        prompt_tokens=prompt_tokens,
        output_tokens=output_tokens,
        model_used=model_used,
        prompt_version=prompt_version,
        collector_errors=collector_errors or [],
    )
