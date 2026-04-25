"""Tests for signals/collect.py — top-level signal collection."""

from __future__ import annotations

import textwrap
from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import patch

from heartbeat.config import load_config
from heartbeat.signals.base import (
    CalendarSignal,
    CollectorError,
    GmailSignal,
    VaultSignal,
)
from heartbeat.signals.collect import collect_signals
from heartbeat.signals.state import TickState


def _write_config(
    tmp_path: Path,
    *,
    calendar: bool = True,
    gmail: bool = True,
    vault: bool = True,
) -> Path:
    cfg_path = tmp_path / "heartbeat.toml"
    cfg_path.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "moe"
            engagement_id = "blr"
            vault_root = "{tmp_path}/vault"

            [signals]
            calendar_enabled = {str(calendar).lower()}
            gmail_enabled = {str(gmail).lower()}
            vault_enabled = {str(vault).lower()}

            [outputs]
            firestore_enabled = false
            """
        )
    )
    (tmp_path / "vault").mkdir(exist_ok=True)
    (tmp_path / "vault" / "n.md").write_text("hello")
    return cfg_path


def _ok_calendar(*args, **kwargs):
    return CalendarSignal(upcoming_events=[]), None


def _ok_gmail(*args, **kwargs):
    return GmailSignal(threads=[]), None


def _ok_vault(*args, **kwargs):
    return VaultSignal(changed_files=[]), {"n.md": "fake-mtime"}, None


def test_collect_signals_runs_all_three(tmp_path: Path) -> None:
    config = load_config(_write_config(tmp_path))
    state = TickState()
    with (
        patch("heartbeat.signals.collect.collect_calendar", side_effect=_ok_calendar),
        patch("heartbeat.signals.collect.collect_gmail", side_effect=_ok_gmail),
        patch("heartbeat.signals.collect.collect_vault_with_mtimes", side_effect=_ok_vault),
    ):
        bundle, mtimes = collect_signals(
            config,
            state,
            now=datetime(2026, 4, 25, tzinfo=UTC),
            token_path=tmp_path / "tok.json",
        )
    assert bundle.calendar is not None
    assert bundle.gmail is not None
    assert bundle.vault is not None
    assert bundle.errors == []
    assert mtimes == {"n.md": "fake-mtime"}


def test_collect_signals_skips_disabled_collectors(tmp_path: Path) -> None:
    config = load_config(_write_config(tmp_path, gmail=False, calendar=False))
    state = TickState()
    with patch("heartbeat.signals.collect.collect_vault_with_mtimes", side_effect=_ok_vault):
        bundle, _ = collect_signals(
            config,
            state,
            now=datetime(2026, 4, 25, tzinfo=UTC),
            token_path=tmp_path / "tok.json",
        )
    assert bundle.calendar is None
    assert bundle.gmail is None
    assert bundle.vault is not None


def test_collect_signals_records_partial_failures(tmp_path: Path) -> None:
    """Calendar fails, Gmail succeeds, Vault succeeds → bundle has the
    successful signals + one error. The whole tick must NOT fail."""
    config = load_config(_write_config(tmp_path))
    state = TickState()

    def fail_calendar(*args, **kwargs):
        return None, CollectorError(
            source="calendar", error_code="api_call_failed", message="boom"
        )

    with (
        patch("heartbeat.signals.collect.collect_calendar", side_effect=fail_calendar),
        patch("heartbeat.signals.collect.collect_gmail", side_effect=_ok_gmail),
        patch("heartbeat.signals.collect.collect_vault_with_mtimes", side_effect=_ok_vault),
    ):
        bundle, _ = collect_signals(
            config,
            state,
            now=datetime(2026, 4, 25, tzinfo=UTC),
            token_path=tmp_path / "tok.json",
        )
    assert bundle.calendar is None
    assert bundle.gmail is not None
    assert bundle.vault is not None
    assert len(bundle.errors) == 1
    assert bundle.errors[0].source == "calendar"


def test_collect_signals_passes_state_mtimes_to_vault(tmp_path: Path) -> None:
    config = load_config(_write_config(tmp_path, calendar=False, gmail=False))
    state = TickState(last_vault_mtimes={"n.md": "previous-mtime"})

    captured: dict = {}

    def capture_vault(*, vault_root: Path, last_mtimes: dict[str, str]):
        captured["last_mtimes"] = dict(last_mtimes)
        return VaultSignal(changed_files=[]), {"n.md": "new-mtime"}, None

    with patch("heartbeat.signals.collect.collect_vault_with_mtimes", side_effect=capture_vault):
        _, mtimes = collect_signals(
            config,
            state,
            now=datetime(2026, 4, 25, tzinfo=UTC),
            token_path=tmp_path / "tok.json",
        )

    assert captured["last_mtimes"] == {"n.md": "previous-mtime"}
    assert mtimes == {"n.md": "new-mtime"}
