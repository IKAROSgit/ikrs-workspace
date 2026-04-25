"""Tests for signals/gmail.py."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import MagicMock

from heartbeat.signals.gmail import _parse_thread, collect_gmail


def _fake_thread(
    *,
    tid: str,
    subject: str = "Hi",
    sender: str = "alice@host",
    date: str = "Sat, 25 Apr 2026 12:00:00 +0000",
    snippet: str = "snip",
    labels: list[str] | None = None,
) -> dict:
    return {
        "id": tid,
        "messages": [
            {
                "labelIds": labels or ["UNREAD"],
                "snippet": snippet,
                "internalDate": "1745582400000",
                "payload": {
                    "headers": [
                        {"name": "Subject", "value": subject},
                        {"name": "From", "value": sender},
                        {"name": "Date", "value": date},
                    ]
                },
            }
        ],
    }


def _fake_service(threads: list[dict]) -> MagicMock:
    """Build a fake Gmail service.

    The real call chain is ``service.users().threads().get(id=...).execute()``
    — the ``id`` arg lives on ``.get(...)``, not on ``.execute()``.
    MagicMock's auto-attribute spec returns the *same* return_value for
    every ``.get(...)`` call regardless of kwargs, so we wire ``.get``
    itself to a side-effect that returns a per-call mock whose
    ``.execute`` yields the matching thread payload.
    """
    fake = MagicMock()
    summaries = [{"id": t["id"]} for t in threads]
    fake.users.return_value.threads.return_value.list.return_value.execute.return_value = {
        "threads": summaries
    }
    by_id = {t["id"]: t for t in threads}

    def fake_get(**kwargs: object) -> MagicMock:
        tid = str(kwargs.get("id", ""))
        per_call = MagicMock()
        per_call.execute.return_value = by_id.get(tid, {})
        return per_call

    fake.users.return_value.threads.return_value.get.side_effect = fake_get
    return fake


def test_collect_gmail_happy_path(tmp_path: Path) -> None:
    threads = [
        _fake_thread(tid="t1", subject="Q1 deck", sender="ceo@x.com", labels=["UNREAD", "STARRED"]),
        _fake_thread(tid="t2", subject="lunch?", sender="bob@y.com", labels=["UNREAD"]),
    ]
    signal, error = collect_gmail(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, 13, 0, tzinfo=UTC),
        lookback_hours=24,
        _service=_fake_service(threads),
    )
    assert error is None
    assert signal is not None
    assert len(signal.threads) == 2
    t1 = signal.threads[0]
    assert t1.subject == "Q1 deck"
    assert t1.is_starred is True
    assert t1.is_unread is True


def test_collect_gmail_missing_headers_uses_defaults(tmp_path: Path) -> None:
    thread = {
        "id": "t1",
        "messages": [
            {
                "labelIds": ["UNREAD"],
                "snippet": "",
                "internalDate": "1745582400000",
                "payload": {"headers": []},
            }
        ],
    }
    signal, error = collect_gmail(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookback_hours=24,
        _service=_fake_service([thread]),
    )
    assert error is None
    assert signal is not None
    assert signal.threads[0].subject == "(no subject)"
    assert signal.threads[0].sender == "(unknown)"


def test_collect_gmail_malformed_date_falls_back_to_internal_date(tmp_path: Path) -> None:
    thread = _fake_thread(tid="t1", date="totally-not-rfc-2822")
    signal, error = collect_gmail(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookback_hours=24,
        _service=_fake_service([thread]),
    )
    assert error is None
    assert signal is not None
    # Malformed Date header → fall back to internalDate (epoch ms).
    # The fake's internalDate=1745582400000 → 2025-04-25 12:00 UTC.
    received = signal.threads[0].received_at
    assert "2025-04-25" in received
    # Also confirm we got an ISO-8601 string with a TZ offset.
    assert "T" in received
    assert any(received.endswith(suffix) for suffix in ("+00:00", "Z")) or "+" in received


def test_collect_gmail_empty_thread_skipped(tmp_path: Path) -> None:
    thread = {"id": "t1", "messages": []}
    signal, error = collect_gmail(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookback_hours=24,
        _service=_fake_service([thread]),
    )
    assert error is None
    assert signal is not None
    assert signal.threads == []


def test_collect_gmail_api_failure_returns_collector_error(tmp_path: Path) -> None:
    fake = MagicMock()
    fake.users.return_value.threads.return_value.list.return_value.execute.side_effect = (
        RuntimeError("rate limited")
    )
    signal, error = collect_gmail(
        token_path=tmp_path / "tok.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookback_hours=24,
        _service=fake,
    )
    assert signal is None
    assert error is not None
    assert error.error_code == "api_call_failed"
    assert "rate limited" in error.message


def test_collect_gmail_missing_token_returns_error(tmp_path: Path) -> None:
    signal, error = collect_gmail(
        token_path=tmp_path / "no-such.json",
        now=datetime(2026, 4, 25, tzinfo=UTC),
        lookback_hours=24,
    )
    assert signal is None
    assert error is not None
    assert error.error_code == "missing_token"


# -------- parser unit tests --------


def test_parse_thread_returns_none_for_empty() -> None:
    assert _parse_thread({"id": "t1", "messages": []}) is None


def test_parse_thread_handles_missing_payload() -> None:
    thread = {
        "id": "t1",
        "messages": [{"labelIds": ["UNREAD"], "snippet": "s", "internalDate": "1745582400000"}],
    }
    parsed = _parse_thread(thread)
    assert parsed is not None
    assert parsed.subject == "(no subject)"
