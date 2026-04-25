"""Output sinks (firestore, telegram, audit) + dispatcher.

Public surface (E.5):
- ``OutputSecrets`` — bundle loaded from secrets.env.
- ``dispatch_outputs(config, secrets, result, *, now, tier)`` — top-level
  best-effort dispatch. Routes a ``TickResult`` to Firestore +
  Telegram + JSONL audit + ``heartbeat_health`` telemetry doc.
- ``DispatchResult`` — what was actually dispatched.
- Per-sink helpers (``write_kanban_task``, ``send_telegram_push``,
  ``append_tick_line`` …) for use by tests and future direct callers.

Adapters lazily import their respective SDKs so the unit-test suite
runs without firebase-admin / requests / etc. needing live credentials.
"""

from heartbeat.outputs.audit import (
    AuditError,
    append_action_line,
    append_tick_line,
    is_action_already_logged,
)
from heartbeat.outputs.dispatch import DispatchResult, dispatch_outputs
from heartbeat.outputs.firestore import (
    FirestoreError,
    get_firestore_client,
    reset_client_cache_for_tests,
    write_heartbeat_health,
    write_kanban_task,
)
from heartbeat.outputs.secrets import OutputSecrets
from heartbeat.outputs.telegram import TelegramError, send_telegram_push

__all__ = [
    "AuditError",
    "DispatchResult",
    "FirestoreError",
    "OutputSecrets",
    "TelegramError",
    "append_action_line",
    "append_tick_line",
    "dispatch_outputs",
    "get_firestore_client",
    "is_action_already_logged",
    "reset_client_cache_for_tests",
    "send_telegram_push",
    "write_heartbeat_health",
    "write_kanban_task",
]
