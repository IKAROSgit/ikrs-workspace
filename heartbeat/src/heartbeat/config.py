"""Config loader for heartbeat.toml + secrets.env.

Two-file split:
- ``heartbeat.toml`` — non-secret, version-controllable, lives at
  ``/etc/ikrs-heartbeat/heartbeat.toml``.
- ``secrets.env`` — secret material (Gemini API key, Telegram token,
  Firebase service-account JSON path), lives at
  ``/etc/ikrs-heartbeat/secrets.env`` with mode ``0600 root:root``.

E.1 ships the loader + validator + dataclass shape. Subsequent sub-phases
populate the secret-using fields lazily when their adapter actually fires
(LLM in E.2, signals in E.3, outputs in E.5).
"""

from __future__ import annotations

import os
import tomllib  # Python 3.11+
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class LlmConfig:
    """LLM adapter selection. Provider-specific knobs live here.

    Tier II ships ``provider="gemini"``. ``provider="claude"`` is reserved
    for first commercial tenant who brings their own ``ANTHROPIC_API_KEY``.
    """

    provider: str = "gemini"
    model: str = "gemini-2.5-pro"
    temperature: float = 0.2
    max_output_tokens: int = 4096


@dataclass(frozen=True)
class SignalsConfig:
    """Which signal collectors to enable on each tick."""

    calendar_enabled: bool = True
    gmail_enabled: bool = True
    vault_enabled: bool = True
    # Look-ahead window for calendar (hours). 24h covers the next day.
    calendar_lookahead_hours: int = 24
    # Look-back window for Gmail unread/starred sweep (hours).
    gmail_lookback_hours: int = 24


@dataclass(frozen=True)
class OutputsConfig:
    """Which outputs to emit on each tick."""

    firestore_enabled: bool = True
    telegram_enabled: bool = True
    audit_enabled: bool = True
    # Firestore project ID (also used by Admin SDK init).
    firestore_project_id: str = ""


@dataclass(frozen=True)
class HeartbeatConfig:
    """Full heartbeat configuration."""

    tenant_id: str
    engagement_id: str
    vault_root: Path
    prompt_version: str = "tick_prompt.v1"
    llm: LlmConfig = field(default_factory=LlmConfig)
    signals: SignalsConfig = field(default_factory=SignalsConfig)
    outputs: OutputsConfig = field(default_factory=OutputsConfig)
    # Where structured tick logs are appended. Inside the vault by convention
    # so it travels with the engagement.
    audit_log_path: Path = field(default_factory=lambda: Path("_memory/heartbeat-log.jsonl"))


_REQUIRED_TOP_LEVEL = ("tenant_id", "engagement_id", "vault_root")


def load_config(path: Path) -> HeartbeatConfig:
    """Parse and validate ``heartbeat.toml``.

    Raises ``FileNotFoundError`` if path missing, ``ValueError`` on schema
    violations.
    """

    if not path.exists():
        raise FileNotFoundError(f"config file not found: {path}")

    with path.open("rb") as fh:
        raw: dict[str, Any] = tomllib.load(fh)

    missing = [k for k in _REQUIRED_TOP_LEVEL if k not in raw]
    if missing:
        raise ValueError(f"missing required keys in {path}: {', '.join(missing)}")

    vault_root = Path(str(raw["vault_root"])).expanduser()

    llm_raw = raw.get("llm", {}) or {}
    llm = LlmConfig(
        provider=str(llm_raw.get("provider", "gemini")),
        model=str(llm_raw.get("model", "gemini-2.5-pro")),
        temperature=float(llm_raw.get("temperature", 0.2)),
        max_output_tokens=int(llm_raw.get("max_output_tokens", 4096)),
    )
    if llm.provider not in {"gemini", "claude"}:
        raise ValueError(
            f"llm.provider must be 'gemini' or 'claude', got {llm.provider!r}"
        )

    signals_raw = raw.get("signals", {}) or {}
    signals = SignalsConfig(
        calendar_enabled=bool(signals_raw.get("calendar_enabled", True)),
        gmail_enabled=bool(signals_raw.get("gmail_enabled", True)),
        vault_enabled=bool(signals_raw.get("vault_enabled", True)),
        calendar_lookahead_hours=int(signals_raw.get("calendar_lookahead_hours", 24)),
        gmail_lookback_hours=int(signals_raw.get("gmail_lookback_hours", 24)),
    )

    outputs_raw = raw.get("outputs", {}) or {}
    outputs = OutputsConfig(
        firestore_enabled=bool(outputs_raw.get("firestore_enabled", True)),
        telegram_enabled=bool(outputs_raw.get("telegram_enabled", True)),
        audit_enabled=bool(outputs_raw.get("audit_enabled", True)),
        firestore_project_id=str(outputs_raw.get("firestore_project_id", "")),
    )
    if outputs.firestore_enabled and not outputs.firestore_project_id:
        raise ValueError(
            "outputs.firestore_project_id is required when outputs.firestore_enabled = true"
        )

    audit_log_path_raw = raw.get("audit_log_path", "_memory/heartbeat-log.jsonl")
    audit_log_path = Path(str(audit_log_path_raw))
    if not audit_log_path.is_absolute():
        # Relative paths are interpreted relative to vault_root for portability.
        audit_log_path = vault_root / audit_log_path

    return HeartbeatConfig(
        tenant_id=str(raw["tenant_id"]),
        engagement_id=str(raw["engagement_id"]),
        vault_root=vault_root,
        prompt_version=str(raw.get("prompt_version", "tick_prompt.v1")),
        llm=llm,
        signals=signals,
        outputs=outputs,
        audit_log_path=audit_log_path,
    )


def env_or(key: str, default: str | None = None) -> str | None:
    """Read a secret from process env (populated from secrets.env at boot).

    Used by adapters for ``GEMINI_API_KEY``, ``TELEGRAM_BOT_TOKEN``, etc.
    """

    return os.environ.get(key, default)
