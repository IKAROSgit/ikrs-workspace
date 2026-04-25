"""Tests for signals/vault.py — filesystem walk + mtime diff."""

from __future__ import annotations

from datetime import datetime
from pathlib import Path

from heartbeat.signals.vault import collect_vault, collect_vault_with_mtimes


def _touch(path: Path, content: str = "x") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    return path


def test_collect_vault_first_run_marks_all_files_added(tmp_path: Path) -> None:
    _touch(tmp_path / "notes" / "a.md", "alpha")
    _touch(tmp_path / "notes" / "b.md", "beta")
    signal, error = collect_vault(vault_root=tmp_path, last_mtimes={})
    assert error is None
    assert signal is not None
    paths = sorted(c.path for c in signal.changed_files)
    assert paths == ["notes/a.md", "notes/b.md"]
    assert all(c.change_type == "added" for c in signal.changed_files)


def test_collect_vault_detects_modifications(tmp_path: Path) -> None:
    f = _touch(tmp_path / "n.md", "v1")
    # Capture initial mtime via a first pass.
    _, mtimes, _ = collect_vault_with_mtimes(vault_root=tmp_path, last_mtimes={})
    assert "n.md" in mtimes

    # Bump mtime.
    import os
    import time

    time.sleep(0.01)  # ensure mtime resolution registers a change
    new_time = datetime.now().timestamp() + 10
    os.utime(f, (new_time, new_time))

    signal, error = collect_vault(vault_root=tmp_path, last_mtimes=mtimes)
    assert error is None
    assert signal is not None
    assert len(signal.changed_files) == 1
    assert signal.changed_files[0].change_type == "modified"
    assert signal.changed_files[0].path == "n.md"


def test_collect_vault_detects_deletions(tmp_path: Path) -> None:
    f = _touch(tmp_path / "deleted.md", "x")
    _, mtimes, _ = collect_vault_with_mtimes(vault_root=tmp_path, last_mtimes={})
    f.unlink()
    signal, error = collect_vault(vault_root=tmp_path, last_mtimes=mtimes)
    assert error is None
    assert signal is not None
    assert len(signal.changed_files) == 1
    assert signal.changed_files[0].change_type == "deleted"
    assert signal.changed_files[0].path == "deleted.md"


def test_collect_vault_skips_ignored_directories(tmp_path: Path) -> None:
    _touch(tmp_path / "real.md")
    _touch(tmp_path / ".git" / "config")
    _touch(tmp_path / ".obsidian" / "workspace.json")
    _touch(tmp_path / "node_modules" / "pkg" / "index.js")
    _touch(tmp_path / "_memory" / "heartbeat-state.json")
    signal, error = collect_vault(vault_root=tmp_path, last_mtimes={})
    assert error is None
    assert signal is not None
    assert sorted(c.path for c in signal.changed_files) == ["real.md"]


def test_collect_vault_skips_temp_files(tmp_path: Path) -> None:
    _touch(tmp_path / "draft.md")
    _touch(tmp_path / "draft.md.tmp")
    _touch(tmp_path / "draft.md.swp")
    _touch(tmp_path / "lock.lock")
    signal, error = collect_vault(vault_root=tmp_path, last_mtimes={})
    assert error is None
    assert signal is not None
    assert sorted(c.path for c in signal.changed_files) == ["draft.md"]


def test_collect_vault_returns_error_when_root_missing(tmp_path: Path) -> None:
    signal, error = collect_vault(vault_root=tmp_path / "no-such", last_mtimes={})
    assert signal is None
    assert error is not None
    assert error.error_code == "vault_root_missing"


def test_collect_vault_returns_error_when_root_is_file(tmp_path: Path) -> None:
    f = _touch(tmp_path / "file.md")
    signal, error = collect_vault(vault_root=f, last_mtimes={})
    assert signal is None
    assert error is not None
    assert error.error_code == "vault_root_missing"


def test_collect_vault_with_mtimes_emits_fresh_snapshot(tmp_path: Path) -> None:
    _touch(tmp_path / "a.md")
    _touch(tmp_path / "sub" / "b.md")
    signal, mtimes, error = collect_vault_with_mtimes(vault_root=tmp_path, last_mtimes={})
    assert error is None
    assert signal is not None
    # Both files should be present in mtimes regardless of change status.
    assert sorted(mtimes.keys()) == ["a.md", "sub/b.md"]


def test_collect_vault_with_mtimes_returns_error_for_missing_root(tmp_path: Path) -> None:
    signal, mtimes, error = collect_vault_with_mtimes(
        vault_root=tmp_path / "no-such",
        last_mtimes={"existing": "t"},
    )
    assert error is not None
    assert error.error_code == "vault_root_missing"
    # On error we preserve the prior mtimes (don't blow away state).
    assert mtimes == {"existing": "t"}


def test_collect_vault_changes_sort_stable(tmp_path: Path) -> None:
    """Order matters for the LLM prompt — we want runs with the same
    inputs to produce identical change lists."""
    _touch(tmp_path / "z.md")
    _touch(tmp_path / "a.md")
    _touch(tmp_path / "m.md")
    signal, error = collect_vault(vault_root=tmp_path, last_mtimes={})
    assert error is None
    assert signal is not None
    paths = [c.path for c in signal.changed_files]
    assert paths == sorted(paths)
