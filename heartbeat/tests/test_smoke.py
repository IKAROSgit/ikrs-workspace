"""Smoke tests for E.1 — package imports + dry-run + config loader."""

from __future__ import annotations

import textwrap
from pathlib import Path

import pytest

import heartbeat
from heartbeat import main as main_mod
from heartbeat.config import HeartbeatConfig, load_config


def test_package_version_is_string() -> None:
    assert isinstance(heartbeat.__version__, str)
    assert heartbeat.__version__ != ""


def _write_minimal_config(tmp_path: Path, *, with_firestore_id: bool = True) -> Path:
    cfg_path = tmp_path / "heartbeat.toml"
    project_line = (
        'firestore_project_id = "ikrs-test"' if with_firestore_id else ""
    )
    cfg_path.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "moe-ikaros-ae"
            engagement_id = "blr-world"
            vault_root = "{tmp_path}/vault"
            prompt_version = "tick_prompt.v1"

            [llm]
            provider = "gemini"
            model = "gemini-2.5-pro"

            [signals]
            calendar_enabled = true
            gmail_enabled = true
            vault_enabled = true

            [outputs]
            firestore_enabled = true
            telegram_enabled = false
            audit_enabled = true
            {project_line}
            """
        )
    )
    return cfg_path


def test_load_config_happy_path(tmp_path: Path) -> None:
    cfg_path = _write_minimal_config(tmp_path)
    cfg = load_config(cfg_path)
    assert isinstance(cfg, HeartbeatConfig)
    assert cfg.tenant_id == "moe-ikaros-ae"
    assert cfg.engagement_id == "blr-world"
    assert cfg.llm.provider == "gemini"
    assert cfg.outputs.telegram_enabled is False
    # Relative audit_log_path resolves under vault_root by default.
    assert cfg.audit_log_path == cfg.vault_root / "_memory/heartbeat-log.jsonl"


def test_load_config_missing_file(tmp_path: Path) -> None:
    with pytest.raises(FileNotFoundError):
        load_config(tmp_path / "does-not-exist.toml")


def test_load_config_missing_required_keys(tmp_path: Path) -> None:
    cfg_path = tmp_path / "broken.toml"
    cfg_path.write_text('tenant_id = "x"\n')  # missing engagement_id, vault_root
    with pytest.raises(ValueError, match="missing required keys"):
        load_config(cfg_path)


def test_load_config_rejects_invalid_provider(tmp_path: Path) -> None:
    cfg_path = tmp_path / "bad-provider.toml"
    cfg_path.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "x"
            engagement_id = "y"
            vault_root = "{tmp_path}/v"
            [llm]
            provider = "openai"
            """
        )
    )
    with pytest.raises(ValueError, match="provider"):
        load_config(cfg_path)


def test_load_config_rejects_firestore_without_project_id(tmp_path: Path) -> None:
    cfg_path = _write_minimal_config(tmp_path, with_firestore_id=False)
    with pytest.raises(ValueError, match="firestore_project_id"):
        load_config(cfg_path)


def test_main_dry_run_returns_zero(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    cfg_path = _write_minimal_config(tmp_path)
    # caplog defaults to WARNING; raise to INFO so we see the dry-run plan.
    with caplog.at_level("INFO", logger="heartbeat.main"):
        rc = main_mod.main(["--dry-run", "--config", str(cfg_path)])
    assert rc == 0
    # Dry-run plan should mention the loaded engagement.
    assert any("blr-world" in rec.getMessage() for rec in caplog.records)


def test_main_real_tick_refuses_until_e4(tmp_path: Path) -> None:
    cfg_path = _write_minimal_config(tmp_path)
    rc = main_mod.main(["--config", str(cfg_path)])
    # E.1 explicitly refuses to run a real tick — must return non-zero so a
    # broken systemd timer does not silently no-op.
    assert rc != 0


def test_main_missing_config_returns_two(tmp_path: Path) -> None:
    rc = main_mod.main(["--dry-run", "--config", str(tmp_path / "nope.toml")])
    assert rc == 2


def test_audit_log_path_traversal_rejected(tmp_path: Path) -> None:
    """A relative ``audit_log_path`` that escapes ``vault_root`` must be
    rejected. Without this guard, a hostile or malformed config could write
    JSONL anywhere the (root-owned) systemd unit has perms — including
    ``/etc/passwd``.
    """

    cfg_path = tmp_path / "traversal.toml"
    cfg_path.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "x"
            engagement_id = "y"
            vault_root = "{tmp_path}/vault"
            audit_log_path = "../../../etc/passwd"

            [outputs]
            firestore_enabled = false
            """
        )
    )
    with pytest.raises(ValueError, match="outside vault_root"):
        load_config(cfg_path)


def test_audit_log_path_absolute_allowed(tmp_path: Path) -> None:
    """Operator-supplied absolute ``audit_log_path`` is trusted — only
    relative paths are containment-checked.
    """

    abs_log = tmp_path / "external.jsonl"
    cfg_path = tmp_path / "abs.toml"
    cfg_path.write_text(
        textwrap.dedent(
            f"""\
            tenant_id = "x"
            engagement_id = "y"
            vault_root = "{tmp_path}/vault"
            audit_log_path = "{abs_log}"

            [outputs]
            firestore_enabled = false
            """
        )
    )
    cfg = load_config(cfg_path)
    assert cfg.audit_log_path == abs_log
