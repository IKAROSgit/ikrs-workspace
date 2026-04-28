"""Calendar signal collector.

Reads next-N-hours events from the operator's primary calendar. N is
``signals.calendar_lookahead_hours`` from heartbeat.toml (default 24).
"""

from __future__ import annotations

import logging
from datetime import datetime, timedelta
from typing import TYPE_CHECKING, Any

from heartbeat.signals.base import (
    CalendarEvent,
    CalendarSignal,
    CollectorError,
)
from heartbeat.signals.firestore_tokens import load_credentials
from heartbeat.signals.google_auth import GoogleAuthFailure

if TYPE_CHECKING:
    from pathlib import Path

logger = logging.getLogger("heartbeat.signals.calendar")


def collect_calendar(
    *,
    token_path: Path,
    now: datetime,
    lookahead_hours: int,
    engagement_id: str = "",
    _service: Any | None = None,
) -> tuple[CalendarSignal | None, CollectorError | None]:
    """Run one calendar collection.

    Returns ``(signal, None)`` on success, ``(None, error)`` on failure.
    Never raises — all paths are converted to a ``CollectorError`` so
    one collector's failure doesn't kill the whole tick.

    ``_service`` is a test seam: pass a fake calendar service to bypass
    the real ``google-api-python-client`` build.
    """

    try:
        if _service is None:
            try:
                creds = load_credentials(engagement_id, token_path, source="calendar")
            except GoogleAuthFailure as exc:
                return None, exc.error

            from googleapiclient.discovery import build  # local import

            _service = build("calendar", "v3", credentials=creds, cache_discovery=False)

        time_min = now.astimezone().isoformat()
        time_max = (now + timedelta(hours=lookahead_hours)).astimezone().isoformat()

        # google-api-python-client raises HttpError + various network
        # exceptions; collapse them all to api_call_failed.
        result = (
            _service.events()
            .list(
                calendarId="primary",
                timeMin=time_min,
                timeMax=time_max,
                maxResults=50,
                singleEvents=True,
                orderBy="startTime",
            )
            .execute()
        )
    except Exception as exc:  # noqa: BLE001 — narrow types vary
        logger.warning("calendar collect failed: %s", exc)
        return None, CollectorError(
            source="calendar",
            error_code="api_call_failed",
            message=f"calendar.events.list failed: {type(exc).__name__}: {exc}",
        )

    items = result.get("items", []) or []
    events = [_parse_event(item) for item in items]
    return CalendarSignal(upcoming_events=events), None


def _parse_event(raw: dict[str, Any]) -> CalendarEvent:
    """Normalise the Google Calendar API event into our flat dataclass."""

    start_block = raw.get("start", {}) or {}
    end_block = raw.get("end", {}) or {}
    is_all_day = "date" in start_block and "dateTime" not in start_block
    start = str(start_block.get("dateTime") or start_block.get("date") or "")
    end = str(end_block.get("dateTime") or end_block.get("date") or "")

    attendees_raw = raw.get("attendees", []) or []
    attendees = [
        str(a.get("email") or a.get("displayName") or "")
        for a in attendees_raw
        if isinstance(a, dict)
    ]
    attendees = [a for a in attendees if a]

    return CalendarEvent(
        id=str(raw.get("id", "")),
        summary=str(raw.get("summary", "(no title)")),
        start=start,
        end=end,
        attendees=attendees,
        location=str(raw["location"]) if raw.get("location") else None,
        is_all_day=is_all_day,
    )
