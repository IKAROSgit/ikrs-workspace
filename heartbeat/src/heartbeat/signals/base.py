"""Signal types — provider-agnostic shapes the tick orchestrator (E.4)
serialises into the prompt, and that the audit log + Firestore reflect.

Every collector returns its own typed signal. The tick assembles them
into a ``SignalsBundle`` so a failure in one collector (e.g. Gmail OAuth
revoked) cannot kill the whole tick — Vault + Calendar still feed the
LLM, and the bundle's ``errors`` list records the partial failure for
telemetry.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

# ---- shared error shape ------------------------------------------------------

ErrorCode = Literal[
    "missing_token",
    "oauth_refresh_failed",
    "network_error",
    "api_call_failed",
    "vault_root_missing",
    "vault_io_error",
    "key_version_unknown",    # Phase F: encryption key version mismatch
    "token_decrypt_failed",   # Phase F: AES-GCM decrypt failure
]


@dataclass(frozen=True)
class CollectorError:
    """One collector's failure. Logged + folded into telemetry — does not
    raise out of ``collect_signals`` so other collectors run regardless."""

    source: Literal["calendar", "gmail", "vault"]
    error_code: ErrorCode
    message: str


# ---- calendar ----------------------------------------------------------------


@dataclass(frozen=True)
class CalendarEvent:
    id: str
    summary: str
    start: str  # ISO-8601 (date or datetime depending on event)
    end: str
    attendees: list[str] = field(default_factory=list)
    location: str | None = None
    is_all_day: bool = False


@dataclass(frozen=True)
class CalendarSignal:
    upcoming_events: list[CalendarEvent] = field(default_factory=list)


# ---- gmail -------------------------------------------------------------------


@dataclass(frozen=True)
class EmailThread:
    id: str
    subject: str
    sender: str  # "Display Name <addr@host>" form, raw from the From header
    snippet: str
    received_at: str  # ISO-8601 from the message Date header
    is_unread: bool
    is_starred: bool


@dataclass(frozen=True)
class GmailSignal:
    threads: list[EmailThread] = field(default_factory=list)


# ---- vault -------------------------------------------------------------------


ChangeType = Literal["added", "modified", "deleted"]


@dataclass(frozen=True)
class VaultFileChange:
    path: str  # POSIX-style, relative to vault_root
    change_type: ChangeType
    mtime: str  # ISO-8601 with TZ; empty for deletions
    size_bytes: int


@dataclass(frozen=True)
class VaultSignal:
    changed_files: list[VaultFileChange] = field(default_factory=list)


# ---- bundle ------------------------------------------------------------------


@dataclass(frozen=True)
class SignalsBundle:
    """All signals collected for one tick, plus any per-collector errors."""

    calendar: CalendarSignal | None = None
    gmail: GmailSignal | None = None
    vault: VaultSignal | None = None
    errors: list[CollectorError] = field(default_factory=list)
