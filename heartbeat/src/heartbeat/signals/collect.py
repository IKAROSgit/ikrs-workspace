"""Top-level signal-collection entry point. Runs all three collectors and
folds their results into a ``SignalsBundle``.

The tick orchestrator (E.4) calls ``collect_signals(...)`` once per tick.
Failures from individual collectors are recorded in ``bundle.errors`` —
the function never raises.
"""

from __future__ import annotations

import logging
from datetime import datetime
from pathlib import Path

from heartbeat.config import HeartbeatConfig
from heartbeat.signals.base import SignalsBundle
from heartbeat.signals.calendar import collect_calendar
from heartbeat.signals.gmail import collect_gmail
from heartbeat.signals.state import TickState
from heartbeat.signals.vault import collect_vault_with_mtimes

logger = logging.getLogger("heartbeat.signals.collect")


def collect_signals(
    config: HeartbeatConfig,
    state: TickState,
    *,
    now: datetime,
    token_path: Path,
) -> tuple[SignalsBundle, dict[str, str]]:
    """Run all enabled collectors. Return ``(bundle, new_vault_mtimes)``.

    The caller persists ``new_vault_mtimes`` into the next ``TickState``
    so subsequent ticks can compute their vault diff.

    Disabled collectors short-circuit silently — their slot in the bundle
    stays ``None``.
    """

    bundle = SignalsBundle()
    new_vault_mtimes = dict(state.last_vault_mtimes)

    if config.signals.calendar_enabled:
        cal_signal, cal_error = collect_calendar(
            token_path=token_path,
            now=now,
            lookahead_hours=config.signals.calendar_lookahead_hours,
            engagement_id=config.engagement_id,
        )
        if cal_error is not None:
            bundle = _with_error(bundle, cal_error)
        if cal_signal is not None:
            bundle = _replace(bundle, calendar=cal_signal)

    if config.signals.gmail_enabled:
        gmail_signal, gmail_error = collect_gmail(
            token_path=token_path,
            now=now,
            lookback_hours=config.signals.gmail_lookback_hours,
            engagement_id=config.engagement_id,
        )
        if gmail_error is not None:
            bundle = _with_error(bundle, gmail_error)
        if gmail_signal is not None:
            bundle = _replace(bundle, gmail=gmail_signal)

    if config.signals.vault_enabled:
        vault_signal, fresh_mtimes, vault_error = collect_vault_with_mtimes(
            vault_root=config.vault_root,
            last_mtimes=state.last_vault_mtimes,
        )
        new_vault_mtimes = fresh_mtimes
        if vault_error is not None:
            bundle = _with_error(bundle, vault_error)
        if vault_signal is not None:
            bundle = _replace(bundle, vault=vault_signal)

    return bundle, new_vault_mtimes


def _with_error(bundle: SignalsBundle, error: object) -> SignalsBundle:
    return SignalsBundle(
        calendar=bundle.calendar,
        gmail=bundle.gmail,
        vault=bundle.vault,
        errors=[*bundle.errors, error],  # type: ignore[list-item]
    )


def _replace(
    bundle: SignalsBundle,
    *,
    calendar: object | None = None,
    gmail: object | None = None,
    vault: object | None = None,
) -> SignalsBundle:
    return SignalsBundle(
        calendar=calendar if calendar is not None else bundle.calendar,  # type: ignore[arg-type]
        gmail=gmail if gmail is not None else bundle.gmail,  # type: ignore[arg-type]
        vault=vault if vault is not None else bundle.vault,  # type: ignore[arg-type]
        errors=bundle.errors,
    )
