"""Vault signal collector.

Walks ``vault_root`` and reports files that changed since the last tick
based on mtime comparison against ``TickState.last_vault_mtimes``.

Intentionally simple: no content hashing, no diffs. The LLM can read
files individually if it wants detail; the tick just tells it which
files moved.
"""

from __future__ import annotations

import logging
from datetime import datetime
from pathlib import Path

from heartbeat.signals.base import (
    CollectorError,
    VaultFileChange,
    VaultSignal,
)

logger = logging.getLogger("heartbeat.signals.vault")


# Directories we never walk into. ``_memory/`` excluded so the heartbeat's
# own state file doesn't show up as a "vault change" each tick.
_IGNORE_DIR_NAMES = frozenset(
    {
        ".git",
        ".obsidian",
        ".trash",
        "node_modules",
        ".venv",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        ".ruff_cache",
        "_memory",
    }
)
# Files we never report on.
_IGNORE_FILE_SUFFIXES = (".tmp", ".swp", ".swo", ".lock")


def collect_vault(
    *,
    vault_root: Path,
    last_mtimes: dict[str, str],
) -> tuple[VaultSignal | None, CollectorError | None]:
    """Walk ``vault_root`` and diff against ``last_mtimes``.

    Returns ``(signal, None)`` on success, ``(None, error)`` on failure.
    Never raises.
    """

    if not vault_root.exists():
        return None, CollectorError(
            source="vault",
            error_code="vault_root_missing",
            message=f"vault_root does not exist: {vault_root}",
        )
    if not vault_root.is_dir():
        return None, CollectorError(
            source="vault",
            error_code="vault_root_missing",
            message=f"vault_root is not a directory: {vault_root}",
        )

    try:
        seen: dict[str, str] = {}
        changes: list[VaultFileChange] = []

        for f in _iter_vault_files(vault_root):
            rel = f.relative_to(vault_root).as_posix()
            try:
                stat = f.stat()
            except OSError as exc:
                # File vanished between iterdir and stat — race with another
                # writer. Skip silently; next tick will catch it.
                logger.debug("skipping %s: %s", f, exc)
                continue

            mtime_iso = (
                datetime.fromtimestamp(stat.st_mtime).astimezone().isoformat()
            )
            seen[rel] = mtime_iso
            prev = last_mtimes.get(rel)

            if prev is None:
                changes.append(
                    VaultFileChange(
                        path=rel,
                        change_type="added",
                        mtime=mtime_iso,
                        size_bytes=stat.st_size,
                    )
                )
            elif prev != mtime_iso:
                changes.append(
                    VaultFileChange(
                        path=rel,
                        change_type="modified",
                        mtime=mtime_iso,
                        size_bytes=stat.st_size,
                    )
                )

        # Detect deletions: anything in last_mtimes but not in seen.
        for prev_rel in last_mtimes:
            if prev_rel not in seen:
                changes.append(
                    VaultFileChange(
                        path=prev_rel,
                        change_type="deleted",
                        mtime="",
                        size_bytes=0,
                    )
                )
    except OSError as exc:
        return None, CollectorError(
            source="vault",
            error_code="vault_io_error",
            message=f"vault walk failed: {type(exc).__name__}: {exc}",
        )

    # Stable order — paths first, then within a path the change_type
    # (added < deleted < modified is fine; sort by path is enough).
    changes.sort(key=lambda c: (c.path, c.change_type))
    return VaultSignal(changed_files=changes), None


def collect_vault_with_mtimes(
    *,
    vault_root: Path,
    last_mtimes: dict[str, str],
) -> tuple[VaultSignal | None, dict[str, str], CollectorError | None]:
    """Collect vault and also return the new mtimes map for state persistence.

    The tick orchestrator (E.4) calls this so it can replace the state
    file's ``last_vault_mtimes`` with the current snapshot — that's how
    next tick computes its diff.
    """

    if not vault_root.exists() or not vault_root.is_dir():
        signal, error = collect_vault(vault_root=vault_root, last_mtimes=last_mtimes)
        return signal, dict(last_mtimes), error

    new_mtimes: dict[str, str] = {}
    try:
        for f in _iter_vault_files(vault_root):
            rel = f.relative_to(vault_root).as_posix()
            try:
                stat = f.stat()
            except OSError:
                continue
            new_mtimes[rel] = (
                datetime.fromtimestamp(stat.st_mtime).astimezone().isoformat()
            )
    except OSError:
        # Re-run the collect path so the error gets a CollectorError.
        signal, error = collect_vault(vault_root=vault_root, last_mtimes=last_mtimes)
        return signal, dict(last_mtimes), error

    signal, error = collect_vault(vault_root=vault_root, last_mtimes=last_mtimes)
    return signal, new_mtimes, error


def _iter_vault_files(root: Path) -> list[Path]:
    """Walk the vault, skipping ignored dirs + files.

    Symlink policy: NEVER follow symlinks. ``Path.is_dir()`` and
    ``Path.is_file()`` follow symlinks by default; without this guard,
    a vault containing ``etc-link -> /etc`` would have heartbeat ingest
    /etc files into the LLM prompt and into ``last_vault_mtimes``. We
    use ``is_symlink()`` as a hard pre-check before either is_dir / is_file
    is consulted. Symlinked files are also skipped — operators who want
    a file in the vault should put it there, not link to it from
    elsewhere on the disk.
    """

    out: list[Path] = []
    stack = [root]
    while stack:
        current = stack.pop()
        try:
            children = list(current.iterdir())
        except OSError as exc:
            logger.debug("cannot iterdir %s: %s", current, exc)
            continue
        for child in children:
            if child.name in _IGNORE_DIR_NAMES:
                continue
            # Hard symlink guard — see docstring.
            if child.is_symlink():
                logger.debug("skipping symlink %s", child)
                continue
            if child.is_dir():
                stack.append(child)
                continue
            if not child.is_file():
                continue
            if child.name.endswith(_IGNORE_FILE_SUFFIXES):
                continue
            out.append(child)
    return out
