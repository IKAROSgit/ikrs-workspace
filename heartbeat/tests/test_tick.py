"""Tests for the tick orchestrator (E.4)."""

from __future__ import annotations

import json
import textwrap
from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from heartbeat.actions import (
    TICK_RESPONSE_SCHEMA,
    KanbanTaskAction,
    MemoryUpdateAction,
    TelegramPushAction,
    parse_actions,
)
from heartbeat.config import load_config
from heartbeat.llm.base import LlmError, LlmRequest, LlmResponse
from heartbeat.signals.base import (
    CalendarSignal,
    CollectorError,
    GmailSignal,
    SignalsBundle,
    VaultSignal,
)
from heartbeat.signals.state import TickState, load_state, save_state
from heartbeat.tick import _pick_error_code, run_tick

# ---------- helpers -------------------------------------------------------


def _config(tmp_path: Path) -> Path:
    cfg = tmp_path / "heartbeat.toml"
    cfg.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "moe"
            engagement_id = "blr"
            vault_root = "{tmp_path}/vault"
            prompt_version = "tick_prompt.v1"

            [llm]
            provider = "gemini"
            model = "gemini-2.5-pro"

            [signals]
            calendar_enabled = false
            gmail_enabled = false
            vault_enabled = true

            [outputs]
            firestore_enabled = false
            telegram_enabled = false
            audit_enabled = false
            """
        )
    )
    (tmp_path / "vault").mkdir()
    (tmp_path / "vault" / "n.md").write_text("hello")
    return cfg


def _mock_llm(text: str | dict, *, tokens: int = 100) -> MagicMock:
    """Build a fake LlmClient whose .generate() returns canned JSON."""
    client = MagicMock()
    body = json.dumps(text) if isinstance(text, dict) else text
    client.generate.return_value = LlmResponse(
        text=body,
        tokens_used=tokens,
        prompt_tokens=tokens // 2,
        output_tokens=tokens // 2,
        model="gemini-2.5-pro",
    )
    return client


# ---------- happy path ----------------------------------------------------


def test_run_tick_happy_path_produces_actions(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    payload = {
        "summary": "Quiet hour, one note worth keeping.",
        "actions": [
            {
                "type": "memory_update",
                "id": "ignored-llm-id",
                "note": "Sarah is back from leave next Monday",
                "tags": ["client", "blr"],
            }
        ],
    }
    client = _mock_llm(payload)

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=datetime(2026, 4, 25, 12, 0, tzinfo=UTC),
        _client=client,
    )

    assert result.status == "ok"
    assert result.actions_emitted == 1
    assert result.error_code is None
    assert result.summary.startswith("Quiet hour")
    assert isinstance(result.actions[0], MemoryUpdateAction)
    # ID was re-keyed server-side (UUID hex is 32 chars).
    assert len(result.actions[0].id) == 32
    assert result.actions[0].id != "ignored-llm-id"
    assert result.tokens_used == 100
    assert result.model_used == "gemini-2.5-pro"


def test_run_tick_no_actions_returns_no_op(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    client = _mock_llm({"summary": "Nothing changed.", "actions": []})

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        _client=client,
    )

    assert result.status == "no-op"
    assert result.actions_emitted == 0
    assert result.error_code is None


def test_run_tick_persists_state(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    state_path = tmp_path / "state.json"
    client = _mock_llm({"summary": "ok", "actions": []})

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=state_path,
        now=datetime(2026, 4, 25, 12, 0, tzinfo=UTC),
        _client=client,
    )
    assert result.status == "no-op"

    loaded = load_state(state_path)
    assert loaded.last_tick_ts == "2026-04-25T12:00:00+00:00"
    assert "n.md" in loaded.last_vault_mtimes


def test_run_tick_passes_response_schema_to_llm(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    client = _mock_llm({"summary": "x", "actions": []})

    run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        _client=client,
    )

    request: LlmRequest = client.generate.call_args.args[0]
    assert request.response_mime_type == "application/json"
    assert request.response_json_schema is not None
    assert "summary" in request.response_json_schema["properties"]


def test_run_tick_threads_recent_action_summaries_into_prompt(tmp_path: Path) -> None:
    """Per E.4 post-code challenge fix: feed back natural-language
    summaries (not opaque hex IDs) so the LLM can compare *content*."""
    config = load_config(_config(tmp_path))
    state_path = tmp_path / "state.json"
    save_state(
        state_path,
        TickState(
            last_tick_ts="2026-04-25T11:00:00+00:00",
            last_action_ids=["hex-id-1", "hex-id-2"],
            last_action_summaries=[
                "kanban_task[high]: Reply to Sarah re Q1 deck",
                "memory_update: BLR client offsite confirmed for May",
            ],
        ),
    )
    client = _mock_llm({"summary": "x", "actions": []})

    run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=state_path,
        now=datetime(2026, 4, 25, 12, 0, tzinfo=UTC),
        _client=client,
    )

    request: LlmRequest = client.generate.call_args.args[0]
    # Natural-language summaries flow into the prompt.
    assert "Reply to Sarah re Q1 deck" in request.prompt
    assert "BLR client offsite confirmed for May" in request.prompt
    # Opaque hex IDs do NOT — the LLM has no memory of those.
    assert "hex-id-1" not in request.prompt
    assert "hex-id-2" not in request.prompt
    assert "2026-04-25T11:00:00+00:00" in request.prompt


def test_run_tick_persists_action_summaries(tmp_path: Path) -> None:
    """After emitting actions, last_action_summaries should hold their
    natural-language one-liners for the next tick's dedupe context."""
    config = load_config(_config(tmp_path))
    state_path = tmp_path / "state.json"
    payload = {
        "summary": "x",
        "actions": [
            {
                "type": "kanban_task",
                "id": "ignored",
                "title": "Ping Sarah",
                "description": "About the deck",
                "priority": "high",
                "rationale": "deadline tomorrow",
            }
        ],
    }
    client = _mock_llm(payload)

    run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=state_path,
        now=datetime(2026, 4, 25, 12, 0, tzinfo=UTC),
        _client=client,
    )

    from heartbeat.signals.state import load_state

    loaded = load_state(state_path)
    assert len(loaded.last_action_summaries) == 1
    assert "Ping Sarah" in loaded.last_action_summaries[0]
    assert loaded.last_action_summaries[0].startswith("kanban_task[high]")


def test_run_tick_stamps_emitted_at_on_actions(tmp_path: Path) -> None:
    """Per E.4 post-code challenge fix #2: every action must carry an
    emitted_at timestamp set at re-key time."""
    config = load_config(_config(tmp_path))
    payload = {
        "summary": "x",
        "actions": [
            {
                "type": "memory_update",
                "id": "ignored",
                "note": "x",
                "tags": [],
            }
        ],
    }
    client = _mock_llm(payload)

    fixed_now = datetime(2026, 4, 25, 12, 0, tzinfo=UTC)
    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=fixed_now,
        _client=client,
    )
    assert len(result.actions) == 1
    assert result.actions[0].emitted_at == fixed_now.isoformat()


# ---------- error paths ---------------------------------------------------


def test_run_tick_handles_corrupt_state_file(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    state_path = tmp_path / "state.json"
    state_path.write_text("{not json")
    client = _mock_llm({"summary": "x", "actions": []})

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=state_path,
        now=datetime(2026, 4, 25, tzinfo=UTC),
        _client=client,
    )

    assert result.status == "error"
    assert result.error_code == "state_load_failed"
    assert result.actions_emitted == 0


def test_run_tick_handles_llm_error(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    client = MagicMock()
    client.generate.side_effect = LlmError("rate limited", error_code="llm_call_failed")

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        _client=client,
    )

    assert result.status == "error"
    assert result.error_code == "llm_call_failed"
    assert result.actions == []


def test_run_tick_handles_invalid_json_from_llm(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    client = _mock_llm("this is not json at all", tokens=42)

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        _client=client,
    )

    assert result.status == "error"
    assert result.error_code == "action_parse_failed"
    # Token usage still recorded — we did get a real response.
    assert result.tokens_used == 42


def test_run_tick_handles_missing_required_keys(tmp_path: Path) -> None:
    config = load_config(_config(tmp_path))
    # Valid JSON but missing both required keys.
    client = _mock_llm({"foo": "bar"})

    result = run_tick(
        config,
        token_path=tmp_path / "tok.json",
        state_path=tmp_path / "state.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        _client=client,
    )

    assert result.status == "error"
    assert result.error_code == "action_parse_failed"


def test_run_tick_records_collector_errors(tmp_path: Path) -> None:
    """Per-collector errors fold into the result; tick still completes
    successfully if the LLM call goes through."""
    config = load_config(_config(tmp_path))
    client = _mock_llm({"summary": "ok despite errors", "actions": []})

    bundle = SignalsBundle(
        calendar=CalendarSignal(),
        gmail=GmailSignal(),
        vault=VaultSignal(),
        errors=[
            CollectorError(
                source="gmail",
                error_code="oauth_refresh_failed",
                message="revoked",
            ),
            CollectorError(
                source="calendar",
                error_code="network_error",
                message="DNS",
            ),
        ],
    )
    with patch(
        "heartbeat.tick.collect_signals",
        return_value=(bundle, {"n.md": "fake-mtime"}),
    ):
        result = run_tick(
            config,
            token_path=tmp_path / "tok.json",
            state_path=tmp_path / "state.json",
            now=datetime(2026, 4, 25, tzinfo=UTC),
            _client=client,
        )

    # oauth_refresh_failed beats network_error per precedence.
    assert result.error_code == "oauth_refresh_failed"
    assert len(result.collector_errors) == 2


# ---------- _pick_error_code ----------------------------------------------


def test_pick_error_code_empty_returns_none() -> None:
    assert _pick_error_code([]) is None


def test_pick_error_code_picks_highest_precedence() -> None:
    errs = [
        CollectorError(source="gmail", error_code="network_error", message=""),
        CollectorError(source="calendar", error_code="oauth_refresh_failed", message=""),
        CollectorError(source="vault", error_code="vault_io_error", message=""),
    ]
    assert _pick_error_code(errs) == "oauth_refresh_failed"


def test_pick_error_code_unknown_codes_treated_as_lowest() -> None:
    errs = [
        CollectorError(source="gmail", error_code="ok-ish-code", message=""),  # type: ignore[arg-type]
        CollectorError(source="calendar", error_code="network_error", message=""),
    ]
    assert _pick_error_code(errs) == "network_error"


# ---------- action parser -------------------------------------------------


def test_parse_actions_kanban_task() -> None:
    summary, actions = parse_actions(
        {
            "summary": "test",
            "actions": [
                {
                    "type": "kanban_task",
                    "id": "x",
                    "title": "Reply to Sarah",
                    "description": "About Q2 deck",
                    "priority": "high",
                    "rationale": "board meeting tomorrow",
                }
            ],
        }
    )
    assert summary == "test"
    assert isinstance(actions[0], KanbanTaskAction)
    assert actions[0].priority == "high"


def test_parse_actions_telegram_push() -> None:
    _, actions = parse_actions(
        {
            "summary": "x",
            "actions": [
                {
                    "type": "telegram_push",
                    "id": "p1",
                    "message": "Sarah replied",
                    "urgency": "urgent",
                }
            ],
        }
    )
    assert isinstance(actions[0], TelegramPushAction)
    assert actions[0].urgency == "urgent"


def test_parse_actions_unknown_type_silently_dropped() -> None:
    _, actions = parse_actions(
        {
            "summary": "x",
            "actions": [
                {"type": "send_email", "id": "x"},  # future schema
                {
                    "type": "memory_update",
                    "id": "real",
                    "note": "still here",
                    "tags": [],
                },
            ],
        }
    )
    assert len(actions) == 1
    assert isinstance(actions[0], MemoryUpdateAction)


def test_parse_actions_invalid_priority_falls_back_to_medium() -> None:
    _, actions = parse_actions(
        {
            "summary": "x",
            "actions": [
                {
                    "type": "kanban_task",
                    "id": "x",
                    "title": "t",
                    "description": "d",
                    "priority": "OMGWTF",
                    "rationale": "r",
                }
            ],
        }
    )
    assert actions[0].priority == "medium"  # type: ignore[union-attr]


def test_parse_actions_action_without_id_dropped() -> None:
    _, actions = parse_actions(
        {
            "summary": "x",
            "actions": [
                {"type": "memory_update", "note": "no id"},  # missing id
                {"type": "memory_update", "id": "ok", "note": "kept"},
            ],
        }
    )
    assert len(actions) == 1


def test_parse_actions_raises_for_missing_summary() -> None:
    with pytest.raises(Exception, match="summary"):
        parse_actions({"actions": []})


def test_parse_actions_raises_for_non_list_actions() -> None:
    with pytest.raises(Exception, match="must be a list"):
        parse_actions({"summary": "x", "actions": "not a list"})


def test_tick_response_schema_has_required_top_level() -> None:
    assert "summary" in TICK_RESPONSE_SCHEMA["properties"]
    assert "actions" in TICK_RESPONSE_SCHEMA["properties"]
    assert "summary" in TICK_RESPONSE_SCHEMA["required"]
    assert "actions" in TICK_RESPONSE_SCHEMA["required"]


# ---------- prompt rendering ----------------------------------------------


def test_render_tick_prompt_handles_empty_bundle() -> None:
    from heartbeat.prompts import load_prompt_template, render_tick_prompt

    template = load_prompt_template()
    rendered = render_tick_prompt(
        template,
        tick_ts="2026-04-25T12:00:00+00:00",
        tenant_id="moe",
        engagement_id="blr",
        last_tick_ts="",
        recent_action_summaries=[],
        calendar_lookahead_hours=24,
        gmail_lookback_hours=24,
        bundle=SignalsBundle(),
    )
    # First-run substitutions:
    assert "(first run)" in rendered
    assert "(none)" in rendered
    # Disabled collectors:
    assert "(not collected this tick)" in rendered


def test_render_tick_prompt_renders_signals() -> None:
    from heartbeat.prompts import load_prompt_template, render_tick_prompt
    from heartbeat.signals.base import (
        CalendarEvent,
        EmailThread,
        VaultFileChange,
    )

    bundle = SignalsBundle(
        calendar=CalendarSignal(
            upcoming_events=[
                CalendarEvent(
                    id="e1",
                    summary="Standup",
                    start="2026-04-26T09:00",
                    end="2026-04-26T09:30",
                    attendees=["a@x"],
                )
            ]
        ),
        gmail=GmailSignal(
            threads=[
                EmailThread(
                    id="t1",
                    subject="Q1 deck",
                    sender="ceo@x",
                    snippet="hi",
                    received_at="2026-04-25T11",
                    is_unread=True,
                    is_starred=False,
                )
            ]
        ),
        vault=VaultSignal(
            changed_files=[
                VaultFileChange(
                    path="notes/a.md",
                    change_type="modified",
                    mtime="2026-04-25T11",
                    size_bytes=100,
                )
            ]
        ),
    )
    template = load_prompt_template()
    rendered = render_tick_prompt(
        template,
        tick_ts="2026-04-25T12:00:00+00:00",
        tenant_id="moe",
        engagement_id="blr",
        last_tick_ts="2026-04-25T11:00:00+00:00",
        recent_action_summaries=["kanban_task[high]: Old task"],
        calendar_lookahead_hours=24,
        gmail_lookback_hours=24,
        bundle=bundle,
    )
    assert "Standup" in rendered
    assert "Q1 deck" in rendered
    assert "notes/a.md" in rendered
    assert "modified" in rendered
    assert "Old task" in rendered


def test_render_tick_prompt_truncates_mass_vault_changes() -> None:
    """Per E.4 post-code challenge fix #6: a 100K-file vault change
    (git checkout, backup restore) MUST NOT blow up the prompt."""
    from heartbeat.prompts import load_prompt_template, render_tick_prompt
    from heartbeat.signals.base import VaultFileChange

    huge = [
        VaultFileChange(
            path=f"notes/file-{i:05d}.md",
            change_type="modified",
            mtime="2026-04-25T11",
            size_bytes=100,
        )
        for i in range(5000)
    ]
    bundle = SignalsBundle(vault=VaultSignal(changed_files=huge))
    template = load_prompt_template()
    rendered = render_tick_prompt(
        template,
        tick_ts="2026-04-25T12:00:00+00:00",
        tenant_id="moe",
        engagement_id="blr",
        last_tick_ts="",
        recent_action_summaries=[],
        calendar_lookahead_hours=24,
        gmail_lookback_hours=24,
        bundle=bundle,
    )
    # First N rendered, rest summarised in a single "(plus M more...)" line.
    assert "(plus 4800 more files changed" in rendered
    # Lower bound check: a 5000-file dump would be ~250KB; our cap keeps
    # the rendered prompt well under that. Hard cap of ~50KB is plenty
    # of headroom over the 200-line limit.
    assert len(rendered) < 50_000


def test_load_prompt_template_unknown_version_raises() -> None:
    from heartbeat.prompts import load_prompt_template

    with pytest.raises(ValueError, match="unknown prompt version"):
        load_prompt_template("tick_prompt.v99")
