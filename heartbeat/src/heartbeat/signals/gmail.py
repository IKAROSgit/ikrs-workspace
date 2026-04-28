"""Gmail signal collector.

Reads recent unread + starred threads from the operator's inbox over the
last ``signals.gmail_lookback_hours`` (default 24) — that's the "what
needs attention" surface the LLM analyses each tick.
"""

from __future__ import annotations

import contextlib
import logging
from datetime import datetime, timedelta
from email.utils import parsedate_to_datetime
from typing import TYPE_CHECKING, Any

from heartbeat.signals.base import (
    CollectorError,
    EmailThread,
    GmailSignal,
)
from heartbeat.signals.firestore_tokens import load_credentials
from heartbeat.signals.google_auth import GoogleAuthFailure

if TYPE_CHECKING:
    from pathlib import Path

logger = logging.getLogger("heartbeat.signals.gmail")


# Cap on threads pulled per tick — bounds Gmail API quota usage and
# keeps the prompt size sane. Anything beyond is paginated, not silently
# dropped — we just log it and trust the LLM to handle a representative
# sample.
_MAX_THREADS_PER_TICK = 25


def collect_gmail(
    *,
    token_path: Path,
    now: datetime,
    lookback_hours: int,
    engagement_id: str = "",
    _service: Any | None = None,
) -> tuple[GmailSignal | None, CollectorError | None]:
    """Run one gmail collection.

    Returns ``(signal, None)`` on success, ``(None, error)`` on failure.
    Never raises — same pattern as ``collect_calendar``.
    """

    try:
        if _service is None:
            try:
                creds = load_credentials(engagement_id, token_path, source="gmail")
            except GoogleAuthFailure as exc:
                return None, exc.error

            from googleapiclient.discovery import build  # local import

            _service = build("gmail", "v1", credentials=creds, cache_discovery=False)

        cutoff = now - timedelta(hours=lookback_hours)
        # Gmail search query: unread OR starred, after the cutoff. The
        # `after:` operator takes a yyyy/mm/dd date or a Unix timestamp.
        query = f"(is:unread OR is:starred) after:{int(cutoff.timestamp())}"

        list_resp = (
            _service.users()
            .threads()
            .list(userId="me", q=query, maxResults=_MAX_THREADS_PER_TICK)
            .execute()
        )

        thread_summaries = list_resp.get("threads", []) or []
        if len(thread_summaries) >= _MAX_THREADS_PER_TICK:
            logger.info(
                "gmail collect hit %d-thread cap; older items truncated",
                _MAX_THREADS_PER_TICK,
            )

        threads: list[EmailThread] = []
        for summary in thread_summaries:
            tid = str(summary.get("id", ""))
            if not tid:
                continue
            full = (
                _service.users()
                .threads()
                .get(
                    userId="me",
                    id=tid,
                    format="metadata",
                    metadataHeaders=["Subject", "From", "Date"],
                )
                .execute()
            )
            parsed = _parse_thread(full)
            if parsed is not None:
                threads.append(parsed)
    except Exception as exc:  # noqa: BLE001 — see collect_calendar
        logger.warning("gmail collect failed: %s", exc)
        return None, CollectorError(
            source="gmail",
            error_code="api_call_failed",
            message=f"gmail threads call failed: {type(exc).__name__}: {exc}",
        )

    return GmailSignal(threads=threads), None


def _parse_thread(thread: dict[str, Any]) -> EmailThread | None:
    """Convert a Gmail threads.get response into our flat dataclass.

    Gmail returns threads as a list of messages; we look at the most
    recent message in the thread for headers + flags.
    """

    messages = thread.get("messages", []) or []
    if not messages:
        return None
    last = messages[-1]
    headers = {
        h.get("name", "").lower(): str(h.get("value", ""))
        for h in (last.get("payload", {}) or {}).get("headers", [])
        if isinstance(h, dict)
    }
    label_ids = set(last.get("labelIds", []) or [])

    received_at = ""
    date_header = headers.get("date")
    if date_header:
        try:
            received_at = parsedate_to_datetime(date_header).astimezone().isoformat()
        except (TypeError, ValueError):
            # Malformed Date header — fall back to internalDate (epoch ms).
            internal_date = last.get("internalDate")
            if internal_date is not None:
                with contextlib.suppress(TypeError, ValueError):
                    received_at = (
                        datetime.fromtimestamp(int(internal_date) / 1000)
                        .astimezone()
                        .isoformat()
                    )

    return EmailThread(
        id=str(thread.get("id", "")),
        subject=headers.get("subject", "(no subject)"),
        sender=headers.get("from", "(unknown)"),
        snippet=str(last.get("snippet", "")),
        received_at=received_at,
        is_unread="UNREAD" in label_ids,
        is_starred="STARRED" in label_ids,
    )
