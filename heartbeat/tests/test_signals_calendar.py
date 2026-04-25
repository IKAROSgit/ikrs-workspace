"""Tests for signals/calendar.py — calendar event collection + parsing."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import MagicMock

from heartbeat.signals.calendar import _parse_event, collect_calendar


def _fake_service(events: list[dict]) -> MagicMock:
    fake = MagicMock()
    fake.events.return_value.list.return_value.execute.return_value = {"items": events}
    return fake


def test_collect_calendar_happy_path(tmp_path: Path) -> None:
    events = [
        {
            "id": "evt1",
            "summary": "Standup",
            "start": {"dateTime": "2026-04-26T09:00:00+04:00"},
            "end": {"dateTime": "2026-04-26T09:30:00+04:00"},
            "attendees": [{"email": "a@b.com"}, {"displayName": "Anon"}],
            "location": "Zoom",
        }
    ]
    signal, error = collect_calendar(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 26, 8, 0, tzinfo=UTC),
        lookahead_hours=24,
        _service=_fake_service(events),
    )
    assert error is None
    assert signal is not None
    assert len(signal.upcoming_events) == 1
    e = signal.upcoming_events[0]
    assert e.id == "evt1"
    assert e.summary == "Standup"
    assert e.attendees == ["a@b.com", "Anon"]
    assert e.location == "Zoom"
    assert e.is_all_day is False


def test_collect_calendar_handles_all_day_event(tmp_path: Path) -> None:
    events = [
        {
            "id": "holiday",
            "summary": "UAE National Day",
            "start": {"date": "2026-12-02"},
            "end": {"date": "2026-12-03"},
        }
    ]
    signal, error = collect_calendar(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 12, 1, tzinfo=UTC),
        lookahead_hours=48,
        _service=_fake_service(events),
    )
    assert error is None
    assert signal is not None
    assert signal.upcoming_events[0].is_all_day is True
    assert signal.upcoming_events[0].start == "2026-12-02"


def test_collect_calendar_handles_empty_calendar(tmp_path: Path) -> None:
    signal, error = collect_calendar(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookahead_hours=24,
        _service=_fake_service([]),
    )
    assert error is None
    assert signal is not None
    assert signal.upcoming_events == []


def test_collect_calendar_missing_summary_falls_back(tmp_path: Path) -> None:
    events = [
        {
            "id": "x",
            "start": {"dateTime": "2026-04-26T09:00:00Z"},
            "end": {"dateTime": "2026-04-26T10:00:00Z"},
        }
    ]
    signal, error = collect_calendar(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 26, tzinfo=UTC),
        lookahead_hours=24,
        _service=_fake_service(events),
    )
    assert error is None
    assert signal is not None
    assert signal.upcoming_events[0].summary == "(no title)"


def test_collect_calendar_api_failure_returns_collector_error(tmp_path: Path) -> None:
    fake = MagicMock()
    fake.events.return_value.list.return_value.execute.side_effect = RuntimeError(
        "API quota exceeded"
    )
    signal, error = collect_calendar(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookahead_hours=24,
        _service=fake,
    )
    assert signal is None
    assert error is not None
    assert error.source == "calendar"
    assert error.error_code == "api_call_failed"
    assert "quota exceeded" in error.message


def test_collect_calendar_oauth_missing_token_returns_error(tmp_path: Path) -> None:
    """No _service override + missing token file → load_google_credentials
    fails, error bubbles out as a CollectorError."""

    signal, error = collect_calendar(
        token_path=tmp_path / "no-such-token.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookahead_hours=24,
    )
    assert signal is None
    assert error is not None
    assert error.source == "calendar"
    assert error.error_code == "missing_token"


# -------- parser unit tests --------


def test_parse_event_handles_filtering_blank_attendees() -> None:
    raw = {
        "id": "x",
        "summary": "S",
        "start": {"dateTime": "t"},
        "end": {"dateTime": "t"},
        "attendees": [{"email": ""}, {"email": "real@host"}, {}, "not-a-dict"],
    }
    event = _parse_event(raw)
    assert event.attendees == ["real@host"]


def test_parse_event_drops_location_if_falsy() -> None:
    raw = {
        "id": "x",
        "summary": "S",
        "start": {"dateTime": "t"},
        "end": {"dateTime": "t"},
        "location": "",
    }
    assert _parse_event(raw).location is None
