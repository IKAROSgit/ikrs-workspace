"""Telegram Bot API push.

Per spec: per-operator bot via BotFather, not a shared bot. The install
script captures bot_token + chat_id into secrets.env. This module only
sends messages — it does not register the bot or set up webhooks.

Webhook handling: ``deleteWebhook`` is a one-time install-script call
(see install.sh). If the token was previously webhooked by another
process, getUpdates would 409. We don't call deleteWebhook here because:
- Production install runs it once at setup time.
- Tests don't need it.
- Calling it on every tick would race with any other process using the
  bot.

Spec: docs/specs/m3-phase-e-autonomous-heartbeat.md §Telegram.
"""

from __future__ import annotations

import contextlib
import logging
from typing import Any

import requests

from heartbeat.actions import TelegramPushAction
from heartbeat.outputs.secrets import OutputSecrets

logger = logging.getLogger("heartbeat.outputs.telegram")


# Conservative timeout — Telegram is usually fast, and we don't want to
# block the tick if the API is degraded.
_TELEGRAM_TIMEOUT_SECONDS = 10


class TelegramError(RuntimeError):
    def __init__(self, message: str, *, error_code: str = "telegram_error") -> None:
        super().__init__(message)
        self.error_code = error_code


def send_telegram_push(
    *,
    secrets: OutputSecrets,
    action: TelegramPushAction,
    _session: Any | None = None,
) -> None:
    """Send one Telegram message to the operator's chat.

    ``_session`` is a test seam — pass a fake ``requests.Session`` whose
    ``.post`` returns a canned response. Production callers omit it.
    """

    if not secrets.telegram_bot_token or not secrets.telegram_chat_id:
        raise TelegramError(
            "telegram bot_token + chat_id required. Run install.sh on "
            "the VM to capture them into secrets.env.",
            error_code="missing_secrets",
        )

    # Compose the message body. Prefix with an urgency emoji so the
    # operator's lock-screen preview is informative without being noisy.
    prefix = {
        "info": "ℹ️",
        "warning": "⚠️",
        "urgent": "🚨",
    }.get(action.urgency, "ℹ️")
    text = f"{prefix} {action.message}"

    url = f"https://api.telegram.org/bot{secrets.telegram_bot_token}/sendMessage"
    payload = {
        "chat_id": secrets.telegram_chat_id,
        "text": text,
        "disable_web_page_preview": True,
    }

    session = _session if _session is not None else requests
    try:
        resp = session.post(url, json=payload, timeout=_TELEGRAM_TIMEOUT_SECONDS)
    except requests.exceptions.RequestException as exc:
        raise TelegramError(
            f"telegram POST failed: {type(exc).__name__}: {exc}",
            error_code="network_error",
        ) from exc

    if not 200 <= resp.status_code < 300:
        # 401 → token revoked, 400 → bad chat_id, 429 → rate limit, etc.
        # We don't retry; next tick gets a fresh shot.
        body_preview = ""
        with contextlib.suppress(Exception):
            body_preview = resp.text[:200]
        if resp.status_code == 401:
            code = "telegram_auth_failed"
        elif resp.status_code == 429:
            code = "rate_limited"
        else:
            code = "api_call_failed"
        raise TelegramError(
            f"telegram returned {resp.status_code}: {body_preview}",
            error_code=code,
        )
