"""Telegram API client for the bot poller.

Minimal: only getUpdates + sendMessage. No bot framework dependency.
"""

from __future__ import annotations

import logging
from typing import Any

import requests

logger = logging.getLogger("heartbeat.poller.telegram")

_API_BASE = "https://api.telegram.org/bot{token}"


class TelegramError(Exception):
    """Non-retryable Telegram API error."""


class TelegramClient:
    """Thin wrapper around Telegram Bot API for polling."""

    def __init__(self, token: str, timeout: int = 30) -> None:
        self._base = _API_BASE.format(token=token)
        self._timeout = timeout
        self._session = requests.Session()  # type: ignore[no-untyped-call,attr-defined]
        self._backoff = 1.0

    def get_updates(
        self,
        offset: int | None = None,
        limit: int = 100,
        allowed_updates: list[str] | None = None,
    ) -> list[dict[str, Any]]:
        """Long-poll for updates. Returns list of update dicts.

        On network error: raises requests.RequestException (caller handles backoff).
        On API error (non-2xx): raises TelegramError.
        """
        params: dict[str, Any] = {"timeout": self._timeout, "limit": limit}
        if offset is not None:
            params["offset"] = offset
        if allowed_updates:
            params["allowed_updates"] = allowed_updates

        resp = self._session.post(  # type: ignore[no-untyped-call]
            f"{self._base}/getUpdates",
            json=params,
            timeout=self._timeout + 10,  # HTTP timeout > long-poll timeout
        )
        resp.raise_for_status()
        data = resp.json()
        if not data.get("ok"):
            raise TelegramError(f"getUpdates failed: {data.get('description', 'unknown')}")
        self._backoff = 1.0  # reset on success
        result: list[dict[str, Any]] = data.get("result", [])
        return result

    def send_message(self, chat_id: int, text: str) -> None:
        """Send a text message. Best-effort — failures are logged, not raised."""
        try:
            resp = self._session.post(  # type: ignore[no-untyped-call]
                f"{self._base}/sendMessage",
                json={"chat_id": chat_id, "text": text},
                timeout=10,
            )
            if resp.status_code != 200:
                logger.warning("sendMessage failed: %s %s", resp.status_code, resp.text[:200])
        except requests.RequestException as exc:  # type: ignore[attr-defined]
            logger.warning("sendMessage network error: %s", exc)

    def get_backoff(self) -> float:
        """Return current backoff delay and increase it for next call."""
        delay = self._backoff
        self._backoff = min(self._backoff * 2, 60.0)
        return delay

    def reset_backoff(self) -> None:
        self._backoff = 1.0
