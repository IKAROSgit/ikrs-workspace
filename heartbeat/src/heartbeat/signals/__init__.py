"""Signal collectors (calendar, gmail, vault) + last-tick state.

Public surface (E.3):
- ``CalendarSignal``, ``GmailSignal``, ``VaultSignal`` — typed signal
  shapes the tick orchestrator (E.4) consumes.
- ``CollectorError`` — typed failure record. One bad collector cannot
  kill the whole tick.
- ``SignalsBundle`` — what ``collect_signals`` returns.
- ``TickState`` — between-tick persistence with schema versioning +
  atomic writes.
- ``collect_signals(config, state, *, now, token_path)`` — runs all
  three collectors and returns a bundle.
"""

from heartbeat.signals.base import (
    CalendarEvent,
    CalendarSignal,
    CollectorError,
    EmailThread,
    GmailSignal,
    SignalsBundle,
    VaultFileChange,
    VaultSignal,
)
from heartbeat.signals.collect import collect_signals
from heartbeat.signals.state import (
    CURRENT_STATE_SCHEMA,
    TickState,
    load_state,
    save_state,
    write_atomic,
)

__all__ = [
    "CURRENT_STATE_SCHEMA",
    "CalendarEvent",
    "CalendarSignal",
    "CollectorError",
    "EmailThread",
    "GmailSignal",
    "SignalsBundle",
    "TickState",
    "VaultFileChange",
    "VaultSignal",
    "collect_signals",
    "load_state",
    "save_state",
    "write_atomic",
]
