"""Tests for signals/state.py — atomic writes + schema migrations."""

from __future__ import annotations

import dataclasses
import json
import os
import threading
import time
from pathlib import Path

import pytest

from heartbeat.signals.state import (
    _MIGRATIONS,
    CURRENT_STATE_SCHEMA,
    TickState,
    load_state,
    save_state,
    write_atomic,
)

# -------- TickState dataclass --------


def test_tick_state_defaults() -> None:
    state = TickState()
    assert state.schema_version == CURRENT_STATE_SCHEMA
    assert state.last_tick_ts is None
    assert state.last_seen_event_ids == []
    assert state.last_seen_thread_ids == []
    assert state.last_action_ids == []
    assert state.last_vault_mtimes == {}


def test_tick_state_is_frozen() -> None:
    state = TickState()
    with pytest.raises(dataclasses.FrozenInstanceError):
        state.last_tick_ts = "2026-04-25T12:00:00+00:00"  # type: ignore[misc]


# -------- load_state --------


def test_load_state_returns_default_when_file_missing(tmp_path: Path) -> None:
    state = load_state(tmp_path / "nonexistent.json")
    assert state == TickState()


def test_load_state_round_trip(tmp_path: Path) -> None:
    path = tmp_path / "state.json"
    original = TickState(
        last_tick_ts="2026-04-25T12:00:00+00:00",
        last_seen_event_ids=["evt1", "evt2"],
        last_vault_mtimes={"notes/a.md": "2026-04-25T11:00:00+00:00"},
    )
    save_state(path, original)
    loaded = load_state(path)
    assert loaded == original


def test_load_state_corrupt_json_raises_helpful_error(tmp_path: Path) -> None:
    path = tmp_path / "broken.json"
    path.write_text("{this is not json")
    with pytest.raises(RuntimeError, match="corrupt state file"):
        load_state(path)


def test_load_state_drops_unknown_keys(tmp_path: Path) -> None:
    """A future-version writer that drops back to v1 should not blow up
    when its extra fields hit the dataclass constructor."""
    path = tmp_path / "future.json"
    payload = {
        "schema_version": CURRENT_STATE_SCHEMA,
        "last_tick_ts": "2026-04-25T12:00:00+00:00",
        "last_seen_event_ids": [],
        "last_seen_thread_ids": [],
        "last_action_ids": [],
        "last_vault_mtimes": {},
        "future_only_field": "we don't know about this",
    }
    path.write_text(json.dumps(payload))
    state = load_state(path)
    assert state.last_tick_ts == "2026-04-25T12:00:00+00:00"


def test_load_state_refuses_higher_schema_version(tmp_path: Path) -> None:
    path = tmp_path / "future.json"
    path.write_text(json.dumps({"schema_version": CURRENT_STATE_SCHEMA + 5}))
    with pytest.raises(RuntimeError, match="Refusing to downgrade"):
        load_state(path)


def test_load_state_applies_v0_to_v1_migration(tmp_path: Path) -> None:
    """Bootstrap migration: pre-versioned dict gets schema_version=1."""
    path = tmp_path / "v0.json"
    # Note: no schema_version key at all → treated as version 0.
    path.write_text(
        json.dumps(
            {
                "last_tick_ts": "2026-04-24T12:00:00+00:00",
                "last_seen_event_ids": ["e1"],
                "last_seen_thread_ids": [],
                "last_action_ids": [],
                "last_vault_mtimes": {},
            }
        )
    )
    state = load_state(path)
    assert state.schema_version == CURRENT_STATE_SCHEMA
    assert state.last_seen_event_ids == ["e1"]


def test_load_state_raises_if_no_migration_path(tmp_path: Path) -> None:
    """Forge a state at a version that has no upgrade slot."""
    path = tmp_path / "orphan.json"
    # Insert a hole in the migration map and write a dict at that version.
    fake_orphan_version = -7
    path.write_text(json.dumps({"schema_version": fake_orphan_version}))
    with pytest.raises(RuntimeError, match="no migration"):
        load_state(path)


def test_v0_migration_in_dispatch_dict() -> None:
    assert 0 in _MIGRATIONS
    upgraded = _MIGRATIONS[0]({"last_tick_ts": None})
    assert upgraded["schema_version"] == 1


# -------- save_state + atomic write --------


def test_save_state_creates_parent_dirs(tmp_path: Path) -> None:
    nested = tmp_path / "deeply" / "nested" / "state.json"
    save_state(nested, TickState(last_tick_ts="now"))
    assert nested.exists()
    assert json.loads(nested.read_text())["last_tick_ts"] == "now"


def test_write_atomic_does_not_leave_tmp_files_on_success(tmp_path: Path) -> None:
    target = tmp_path / "out.json"
    write_atomic(target, '{"k": "v"}')
    assert target.read_text() == '{"k": "v"}'
    # No tmp leftovers.
    siblings = [p for p in tmp_path.iterdir() if p != target]
    assert siblings == []


def test_write_atomic_unlinks_tmp_on_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """If os.replace blows up, the tmp file must be cleaned up so we
    don't leave detritus accumulating in _memory/ over time."""

    target = tmp_path / "out.json"

    def boom(src: str, dst: str) -> None:
        raise OSError("simulated rename failure")

    monkeypatch.setattr(os, "replace", boom)
    with pytest.raises(OSError, match="simulated rename failure"):
        write_atomic(target, "payload")

    # No tmp leftovers.
    siblings = list(tmp_path.iterdir())
    assert siblings == []


def test_write_atomic_concurrent_writers_do_not_corrupt(tmp_path: Path) -> None:
    """Hammer the same file from many threads — every read must yield
    valid JSON. (A non-atomic implementation would let a reader catch a
    half-written file.)"""

    target = tmp_path / "concurrent.json"
    target.write_text('{"schema_version": 1}')

    errors: list[Exception] = []

    def writer(seed: int) -> None:
        try:
            for i in range(20):
                write_atomic(target, json.dumps({"schema_version": 1, "seed": seed, "i": i}))
        except Exception as exc:  # noqa: BLE001
            errors.append(exc)

    def reader() -> None:
        try:
            for _ in range(40):
                payload = target.read_text()
                json.loads(payload)  # must always parse
                time.sleep(0)
        except Exception as exc:  # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=writer, args=(s,)) for s in range(4)]
    threads += [threading.Thread(target=reader) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert not errors, f"concurrent write/read corrupted state: {errors}"


def test_write_atomic_skips_fsync_when_target_unwritable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Permission denied on tempfile creation should bubble up cleanly."""

    def deny(*args: object, **kwargs: object) -> None:
        raise PermissionError("simulated denied")

    import tempfile

    monkeypatch.setattr(tempfile, "mkstemp", deny)
    with pytest.raises(PermissionError):
        write_atomic(tmp_path / "x.json", "payload")


# -------- save+load integration --------


def test_save_then_load_preserves_all_fields(tmp_path: Path) -> None:
    path = tmp_path / "round.json"
    state = TickState(
        last_tick_ts="2026-04-25T12:00:00+00:00",
        last_seen_event_ids=["a", "b"],
        last_seen_thread_ids=["t1"],
        last_action_ids=["act1"],
        last_vault_mtimes={"a.md": "2026-04-25T11:00:00+00:00"},
    )
    save_state(path, state)
    assert load_state(path) == state


# -------- stdin-style JSON layout (for ops debugging) --------


def test_saved_file_is_human_readable(tmp_path: Path) -> None:
    path = tmp_path / "pretty.json"
    save_state(path, TickState(last_tick_ts="now"))
    text = path.read_text()
    # Pretty-printed (indent=2) and stable key order so diffs are clean.
    assert "  " in text
    assert text.index("\n") > 0
