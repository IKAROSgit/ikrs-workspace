"""Tests for the outputs layer (E.5).

Strategy: every test patches the underlying SDK / network so no live
calls happen. Audit log tests use real tmp_path filesystem because
that path is local-only (no external surface to mock).
"""

from __future__ import annotations

import json
import textwrap
from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from heartbeat.actions import (
    KanbanTaskAction,
    MemoryUpdateAction,
    TelegramPushAction,
)
from heartbeat.config import load_config
from heartbeat.outputs import (
    OutputSecrets,
    append_action_line,
    append_tick_line,
    dispatch_outputs,
    is_action_already_logged,
    reset_client_cache_for_tests,
    send_telegram_push,
    write_heartbeat_health,
    write_kanban_task,
)
from heartbeat.outputs.dispatch import _make_tick_id
from heartbeat.outputs.firestore import FirestoreError, get_firestore_client
from heartbeat.outputs.telegram import TelegramError
from heartbeat.signals.base import CollectorError
from heartbeat.telemetry import HeartbeatHealthDoc
from heartbeat.tick import TickResult

# ---------- helpers -------------------------------------------------------


@pytest.fixture(autouse=True)
def _reset_firestore_cache() -> None:
    reset_client_cache_for_tests()


def _config_path(
    tmp_path: Path,
    *,
    firestore: bool = True,
    telegram: bool = True,
    audit: bool = True,
) -> Path:
    cfg = tmp_path / "heartbeat.toml"
    cfg.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "moe"
            engagement_id = "blr"
            vault_root = "{tmp_path}/vault"

            [outputs]
            firestore_enabled = {str(firestore).lower()}
            telegram_enabled = {str(telegram).lower()}
            audit_enabled = {str(audit).lower()}
            firestore_project_id = "ikrs-test"
            """
        )
    )
    (tmp_path / "vault").mkdir(exist_ok=True)
    return cfg


def _make_secrets(tmp_path: Path) -> OutputSecrets:
    sa_path = tmp_path / "sa.json"
    sa_path.write_text("{}")
    return OutputSecrets(
        firestore_credentials_path=sa_path,
        telegram_bot_token="bot:token",
        telegram_chat_id="12345",
    )


def _kanban_action(action_id: str = "abc") -> KanbanTaskAction:
    return KanbanTaskAction(
        type="kanban_task",
        id=action_id,
        title="Reply to Sarah",
        description="Q1 deck status",
        priority="high",
        rationale="board meeting tomorrow",
        emitted_at="2026-04-25T12:00:00+00:00",
    )


def _telegram_action(action_id: str = "p1") -> TelegramPushAction:
    return TelegramPushAction(
        type="telegram_push",
        id=action_id,
        message="ping",
        urgency="urgent",
        emitted_at="2026-04-25T12:00:00+00:00",
    )


def _memory_action(action_id: str = "m1") -> MemoryUpdateAction:
    return MemoryUpdateAction(
        type="memory_update",
        id=action_id,
        note="Sarah back on Monday",
        tags=["client"],
        emitted_at="2026-04-25T12:00:00+00:00",
    )


def _make_tick_result(
    status: str = "ok",
    actions: list | None = None,
    error_code: str | None = None,
    collector_errors: list[CollectorError] | None = None,
) -> TickResult:
    actions = actions if actions is not None else []
    return TickResult(
        status=status,  # type: ignore[arg-type]
        duration_ms=1000,
        actions_emitted=len(actions),
        error_code=error_code,
        summary="test",
        actions=actions,
        tokens_used=200,
        prompt_tokens=150,
        output_tokens=50,
        model_used="gemini-2.5-pro",
        prompt_version="tick_prompt.v1",
        collector_errors=collector_errors or [],
    )


# ---------- OutputSecrets.from_env ---------------------------------------


def test_secrets_from_env_picks_up_firebase_sa_key(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("FIREBASE_SA_KEY_PATH", "/tmp/sa.json")
    monkeypatch.setenv("TELEGRAM_BOT_TOKEN", "tok")
    monkeypatch.setenv("TELEGRAM_CHAT_ID", "42")
    s = OutputSecrets.from_env()
    assert s.firestore_credentials_path == Path("/tmp/sa.json")
    assert s.telegram_bot_token == "tok"
    assert s.telegram_chat_id == "42"


def test_secrets_from_env_falls_back_to_google_application_credentials(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("FIREBASE_SA_KEY_PATH", raising=False)
    monkeypatch.setenv("GOOGLE_APPLICATION_CREDENTIALS", "/x/y.json")
    s = OutputSecrets.from_env()
    assert s.firestore_credentials_path == Path("/x/y.json")


def test_secrets_from_env_returns_none_when_unset(monkeypatch: pytest.MonkeyPatch) -> None:
    for k in (
        "FIREBASE_SA_KEY_PATH",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "TELEGRAM_BOT_TOKEN",
        "TELEGRAM_CHAT_ID",
    ):
        monkeypatch.delenv(k, raising=False)
    s = OutputSecrets.from_env()
    assert s.firestore_credentials_path is None
    assert s.telegram_bot_token is None
    assert s.telegram_chat_id is None


# ---------- Firestore writers --------------------------------------------


def test_get_firestore_client_raises_without_credentials(tmp_path: Path) -> None:
    secrets = OutputSecrets(None, None, None)
    with pytest.raises(FirestoreError) as exc_info:
        get_firestore_client(secrets)
    assert exc_info.value.error_code == "missing_credentials"


def test_write_kanban_task_serialises_full_doc(tmp_path: Path) -> None:
    """Per E.5 post-code challenge: doc shape must match Tauri's Task type
    (src/types/index.ts:99) so the existing Kanban listeners pick up
    heartbeat-emitted cards without client-side schema migration."""
    fake_client = MagicMock()
    write_kanban_task(
        tenant_id="moe",
        engagement_id="blr",
        action=_kanban_action("kt1"),
        client=fake_client,
    )
    # Collection name is namespaced — "ikrs_tasks" not "tasks".
    fake_client.collection.assert_called_once_with("ikrs_tasks")

    call = fake_client.collection.return_value.document.return_value.set.call_args
    doc = call.args[0]
    # Tauri Task fields:
    assert doc["_v"] == 1
    assert doc["id"] == "kt1"
    assert doc["engagementId"] == "blr"
    assert doc["title"] == "Reply to Sarah"
    assert doc["status"] == "backlog"
    # heartbeat priority "high" → Tauri "p2"
    assert doc["priority"] == "p2"
    assert doc["tags"] == ["heartbeat", "tier-ii"]
    assert doc["subtasks"] == []
    assert doc["source"] == "claude"
    assert doc["createdAt"] == "2026-04-25T12:00:00+00:00"
    assert doc["updatedAt"] == "2026-04-25T12:00:00+00:00"
    # Heartbeat-specific extras (not on Tauri's Task type but harmless):
    assert doc["tenantId"] == "moe"
    assert doc["rationale"] == "board meeting tomorrow"
    assert call.kwargs == {"merge": False}


def test_write_kanban_task_priority_mapping() -> None:
    """Verify the heartbeat → Tauri priority mapping covers all four
    heartbeat values."""
    fake_client = MagicMock()
    cases = [("urgent", "p1"), ("high", "p2"), ("medium", "p3"), ("low", "p3")]
    for hb_priority, expected_tauri in cases:
        action = KanbanTaskAction(
            type="kanban_task",
            id=f"id-{hb_priority}",
            title="t",
            description="d",
            priority=hb_priority,  # type: ignore[arg-type]
            rationale="r",
            emitted_at="t",
        )
        write_kanban_task(
            tenant_id="moe",
            engagement_id="blr",
            action=action,
            client=fake_client,
        )
        last_call = (
            fake_client.collection.return_value.document.return_value.set.call_args
        )
        assert last_call.args[0]["priority"] == expected_tauri


def test_write_kanban_task_wraps_sdk_exception() -> None:
    fake_client = MagicMock()
    fake_client.collection.return_value.document.return_value.set.side_effect = (
        RuntimeError("permission denied")
    )
    with pytest.raises(FirestoreError) as exc_info:
        write_kanban_task(
            tenant_id="moe",
            engagement_id="blr",
            action=_kanban_action(),
            client=fake_client,
        )
    assert exc_info.value.error_code == "task_write_failed"
    assert "permission denied" in str(exc_info.value)


def test_write_heartbeat_health_writes_full_telemetry_doc() -> None:
    fake_client = MagicMock()
    doc = HeartbeatHealthDoc(
        tenantId="moe",
        engagementId="blr",
        tier="II",
        tickTs="2026-04-25T12:00:00+00:00",
        status="ok",
        durationMs=1000,
        tokensUsed=200,
        promptVersion="tick_prompt.v1",
        actionsEmitted=2,
        errorCode=None,
        expiresAt="2026-05-25T12:00:00+00:00",
    )
    write_heartbeat_health(doc=doc, tick_id="moe__blr__T", client=fake_client)
    call = fake_client.collection.return_value.document.return_value.set.call_args
    persisted = call.args[0]
    assert persisted["tier"] == "II"
    assert persisted["tickTs"] == "2026-04-25T12:00:00+00:00"
    assert persisted["actionsEmitted"] == 2


# ---------- Telegram pusher -----------------------------------------------


def test_send_telegram_push_happy_path(tmp_path: Path) -> None:
    secrets = _make_secrets(tmp_path)
    fake_session = MagicMock()
    fake_session.post.return_value = MagicMock(status_code=200, text="{}")
    send_telegram_push(
        secrets=secrets,
        action=_telegram_action(),
        _session=fake_session,
    )
    call = fake_session.post.call_args
    assert "api.telegram.org/botbot:token/sendMessage" in call.args[0]
    assert call.kwargs["json"]["chat_id"] == "12345"
    assert "🚨" in call.kwargs["json"]["text"]


def test_send_telegram_push_missing_secrets_raises() -> None:
    bare = OutputSecrets(None, None, None)
    with pytest.raises(TelegramError) as exc_info:
        send_telegram_push(secrets=bare, action=_telegram_action())
    assert exc_info.value.error_code == "missing_secrets"


def test_send_telegram_push_401_classified_as_auth_failed(tmp_path: Path) -> None:
    secrets = _make_secrets(tmp_path)
    fake_session = MagicMock()
    fake_session.post.return_value = MagicMock(status_code=401, text="unauthorized")
    with pytest.raises(TelegramError) as exc_info:
        send_telegram_push(
            secrets=secrets,
            action=_telegram_action(),
            _session=fake_session,
        )
    assert exc_info.value.error_code == "telegram_auth_failed"


def test_send_telegram_push_429_classified_as_rate_limited(tmp_path: Path) -> None:
    secrets = _make_secrets(tmp_path)
    fake_session = MagicMock()
    fake_session.post.return_value = MagicMock(status_code=429, text="too many")
    with pytest.raises(TelegramError) as exc_info:
        send_telegram_push(
            secrets=secrets,
            action=_telegram_action(),
            _session=fake_session,
        )
    assert exc_info.value.error_code == "rate_limited"


def test_send_telegram_push_network_error_classified(tmp_path: Path) -> None:
    import requests as real_requests

    secrets = _make_secrets(tmp_path)
    fake_session = MagicMock()
    fake_session.post.side_effect = real_requests.ConnectionError("timeout")
    with pytest.raises(TelegramError) as exc_info:
        send_telegram_push(
            secrets=secrets,
            action=_telegram_action(),
            _session=fake_session,
        )
    assert exc_info.value.error_code == "network_error"


def test_send_telegram_push_urgency_emoji_varies(tmp_path: Path) -> None:
    secrets = _make_secrets(tmp_path)
    fake_session = MagicMock()
    fake_session.post.return_value = MagicMock(status_code=200, text="{}")

    for urgency, emoji in [("info", "ℹ️"), ("warning", "⚠️"), ("urgent", "🚨")]:
        action = TelegramPushAction(
            type="telegram_push",
            id="x",
            message="msg",
            urgency=urgency,  # type: ignore[arg-type]
            emitted_at="t",
        )
        fake_session.post.reset_mock()
        send_telegram_push(secrets=secrets, action=action, _session=fake_session)
        text = fake_session.post.call_args.kwargs["json"]["text"]
        assert emoji in text


# ---------- Audit log -----------------------------------------------------


def test_append_tick_line_writes_jsonl(tmp_path: Path) -> None:
    log = tmp_path / "h.jsonl"
    append_tick_line(
        audit_log_path=log,
        tenant_id="moe",
        engagement_id="blr",
        tick_ts="2026-04-25T12:00:00+00:00",
        status="ok",
        duration_ms=1000,
        actions_emitted=2,
        error_code=None,
        summary="Quiet hour.",
        collector_errors=[
            CollectorError(source="gmail", error_code="network_error", message="dns")
        ],
        tokens_used=200,
        prompt_version="tick_prompt.v1",
    )
    lines = log.read_text().splitlines()
    assert len(lines) == 1
    parsed = json.loads(lines[0])
    assert parsed["kind"] == "tick"
    assert parsed["status"] == "ok"
    assert parsed["collectorErrors"][0]["errorCode"] == "network_error"


def test_append_action_line_for_each_type(tmp_path: Path) -> None:
    log = tmp_path / "h.jsonl"
    for action in (_kanban_action(), _memory_action(), _telegram_action()):
        append_action_line(
            audit_log_path=log,
            tenant_id="moe",
            engagement_id="blr",
            action=action,
        )
    lines = [json.loads(line) for line in log.read_text().splitlines()]
    assert len(lines) == 3
    types = {line["type"] for line in lines}
    assert types == {"kanban_task", "memory_update", "telegram_push"}
    # Each line carries dispatchStatus default "ok".
    assert all(line["dispatchStatus"] == "ok" for line in lines)


def test_is_action_already_logged_returns_true_after_append(tmp_path: Path) -> None:
    log = tmp_path / "h.jsonl"
    assert not is_action_already_logged(log, "abc")
    append_action_line(
        audit_log_path=log,
        tenant_id="moe",
        engagement_id="blr",
        action=_kanban_action("abc"),
    )
    assert is_action_already_logged(log, "abc")
    assert not is_action_already_logged(log, "different")


def test_truncates_long_strings(tmp_path: Path) -> None:
    log = tmp_path / "h.jsonl"
    long_summary = "x" * 5000
    append_tick_line(
        audit_log_path=log,
        tenant_id="moe",
        engagement_id="blr",
        tick_ts="t",
        status="ok",
        duration_ms=0,
        actions_emitted=0,
        error_code=None,
        summary=long_summary,
        collector_errors=[],
        tokens_used=0,
        prompt_version="v1",
    )
    parsed = json.loads(log.read_text().splitlines()[0])
    assert parsed["summary"].endswith("…")
    assert len(parsed["summary"]) < len(long_summary)


# ---------- Top-level dispatch -------------------------------------------


def test_dispatch_outputs_skips_actions_when_status_error(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(
        status="error",
        actions=[_kanban_action()],
        error_code="llm_call_failed",
    )
    with patch("heartbeat.outputs.dispatch.write_kanban_task") as wk:
        dispatch = dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    wk.assert_not_called()
    assert dispatch.actions_dispatched == 0
    assert dispatch.actions_failed == 0


def test_dispatch_outputs_routes_each_action_type(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(
        status="ok",
        actions=[_kanban_action("k1"), _memory_action("m1"), _telegram_action("p1")],
    )
    fake_client = MagicMock()
    with (
        patch("heartbeat.outputs.dispatch.get_firestore_client", return_value=fake_client),
        patch("heartbeat.outputs.dispatch.write_kanban_task") as wk,
        patch("heartbeat.outputs.dispatch.send_telegram_push") as st,
        patch("heartbeat.outputs.dispatch.write_heartbeat_health") as wh,
    ):
        dispatch = dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    assert wk.call_count == 1  # one kanban task
    assert st.call_count == 1  # one telegram push
    assert wh.call_count == 1  # one heartbeat_health doc
    # Memory updates dispatch via audit log only — no Firestore/Telegram.
    assert dispatch.actions_dispatched == 3
    assert dispatch.actions_failed == 0


def test_dispatch_outputs_dedupes_on_action_id(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path))
    secrets = _make_secrets(tmp_path)
    # Pre-populate audit log with one action ID — simulates a state-save
    # failure on a previous tick that already logged the action.
    append_action_line(
        audit_log_path=config.audit_log_path,
        tenant_id="moe",
        engagement_id="blr",
        action=_kanban_action("dedupe-me"),
    )
    result = _make_tick_result(
        status="ok",
        actions=[_kanban_action("dedupe-me"), _kanban_action("fresh")],
    )
    with (
        patch(
            "heartbeat.outputs.dispatch.get_firestore_client",
            return_value=MagicMock(),
        ),
        patch("heartbeat.outputs.dispatch.write_kanban_task") as wk,
        patch("heartbeat.outputs.dispatch.write_heartbeat_health"),
    ):
        dispatch = dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    # Only the fresh action got dispatched.
    assert wk.call_count == 1
    assert dispatch.actions_skipped_dedupe == 1
    assert dispatch.actions_dispatched == 1


def test_dispatch_outputs_records_per_action_failure(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(status="ok", actions=[_kanban_action()])
    with (
        patch(
            "heartbeat.outputs.dispatch.get_firestore_client",
            return_value=MagicMock(),
        ),
        patch(
            "heartbeat.outputs.dispatch.write_kanban_task",
            side_effect=FirestoreError("rate limited", error_code="task_write_failed"),
        ),
        patch("heartbeat.outputs.dispatch.write_heartbeat_health"),
    ):
        dispatch = dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    assert dispatch.actions_failed == 1
    assert dispatch.actions_dispatched == 0
    # Audit log has the action with dispatchStatus="error".
    lines = [
        json.loads(line)
        for line in config.audit_log_path.read_text().splitlines()
    ]
    action_lines = [line for line in lines if line["kind"] == "action"]
    assert action_lines[0]["dispatchStatus"] == "error"
    assert "task_write_failed" in action_lines[0]["dispatchError"]


def test_dispatch_outputs_writes_telemetry_doc(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(status="ok")
    fake_client = MagicMock()
    with (
        patch(
            "heartbeat.outputs.dispatch.get_firestore_client", return_value=fake_client
        ),
        patch("heartbeat.outputs.dispatch.write_heartbeat_health") as wh,
    ):
        dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, 12, 0, tzinfo=UTC),
        )
    call = wh.call_args
    doc: HeartbeatHealthDoc = call.kwargs["doc"]
    assert doc.tenantId == "moe"
    assert doc.engagementId == "blr"
    assert doc.tier == "II"
    assert doc.status == "ok"
    # 30-day TTL per spec.
    assert doc.expiresAt > doc.tickTs


def test_dispatch_outputs_audit_disabled_skips_audit(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path, audit=False))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(status="ok", actions=[_memory_action()])
    with (
        patch(
            "heartbeat.outputs.dispatch.get_firestore_client", return_value=MagicMock()
        ),
        patch("heartbeat.outputs.dispatch.write_heartbeat_health"),
    ):
        dispatch = dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    assert dispatch.audit_lines_written == 0
    # audit log file was never touched.
    assert not config.audit_log_path.exists()


def test_dispatch_outputs_firestore_disabled_skips_kanban_and_telemetry(
    tmp_path: Path,
) -> None:
    config = load_config(_config_path(tmp_path, firestore=False))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(
        status="ok", actions=[_kanban_action(), _memory_action()]
    )
    with patch("heartbeat.outputs.dispatch.write_kanban_task") as wk:
        dispatch = dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    wk.assert_not_called()
    assert dispatch.telemetry_written is False


def test_dispatch_outputs_telegram_disabled_skips_push(tmp_path: Path) -> None:
    config = load_config(_config_path(tmp_path, telegram=False))
    secrets = _make_secrets(tmp_path)
    result = _make_tick_result(status="ok", actions=[_telegram_action()])
    with (
        patch(
            "heartbeat.outputs.dispatch.get_firestore_client", return_value=MagicMock()
        ),
        patch("heartbeat.outputs.dispatch.send_telegram_push") as st,
        patch("heartbeat.outputs.dispatch.write_heartbeat_health"),
    ):
        dispatch_outputs(
            config,
            secrets,
            result,
            now=datetime(2026, 4, 25, tzinfo=UTC),
        )
    st.assert_not_called()


# ---------- _make_tick_id deterministic ----------------------------------


def test_make_tick_id_deterministic() -> None:
    now = datetime(2026, 4, 25, 12, 0, tzinfo=UTC)
    assert _make_tick_id("moe", "blr", now) == _make_tick_id("moe", "blr", now)


def test_make_tick_id_safe_for_firestore() -> None:
    now = datetime(2026, 4, 25, 12, 0, tzinfo=UTC)
    tick_id = _make_tick_id("moe", "blr", now)
    # Firestore doc IDs disallow "/", and we replace ":" for URL safety.
    assert "/" not in tick_id
    assert ":" not in tick_id
