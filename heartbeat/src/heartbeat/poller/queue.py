"""Firestore command queue writer.

Writes incoming Telegram commands to engagements/{eid}/commands/{update_id}.
Doc ID is the Telegram update_id (stringified) for natural idempotency.
"""

from __future__ import annotations

import logging
import os
from typing import Any

logger = logging.getLogger("heartbeat.poller.queue")

# Cached Firestore client (same pattern as firestore_tokens.py)
_FS_CLIENT: Any | None = None


def _get_db() -> Any:
    """Return a Firestore client, initializing a named app if needed."""
    global _FS_CLIENT  # noqa: PLW0603
    if _FS_CLIENT is not None:
        return _FS_CLIENT

    import firebase_admin
    from firebase_admin import credentials
    from firebase_admin import firestore as fs

    sa_path = os.environ.get("FIREBASE_SA_KEY_PATH", "/etc/ikrs-heartbeat/firebase-sa.json")
    app_name = "heartbeat-poller"
    try:
        app = firebase_admin.get_app(app_name)  # type: ignore[no-untyped-call]
    except ValueError:
        cred = credentials.Certificate(sa_path)  # type: ignore[no-untyped-call]
        app = firebase_admin.initialize_app(cred, name=app_name)  # type: ignore[no-untyped-call]

    _FS_CLIENT = fs.client(app=app)
    return _FS_CLIENT


def classify_message(message: dict[str, Any]) -> tuple[str, str, str | None]:
    """Classify a Telegram message into (type, payload, snooze_duration).

    Returns:
        type: "text" | "voice" | "confirm" | "snooze" | "dismiss"
        payload: message text, file_id, or action_id
        snooze_duration: only for type="snooze", e.g. "2h"
    """
    # Voice message
    if "voice" in message:
        return "voice", message["voice"].get("file_id", ""), None

    text = (message.get("text") or "").strip()
    if not text:
        return "text", "", None

    lower = text.lower()

    if lower.startswith("/confirm "):
        return "confirm", text[9:].strip(), None

    if lower.startswith("/snooze "):
        parts = text[8:].strip().split(maxsplit=1)
        action_id = parts[0] if parts else ""
        duration = parts[1] if len(parts) > 1 else "1h"
        return "snooze", action_id, duration

    if lower.startswith("/dismiss "):
        return "dismiss", text[9:].strip(), None

    if lower.startswith("/ask "):
        return "text", text[5:].strip(), None

    # Plain text → treat as /ask
    return "text", text, None


def write_command(
    engagement_id: str,
    update_id: int,
    msg_type: str,
    payload: str,
    chat_id: int,
    message_id: int,
    snooze_duration: str | None = None,
    *,
    _db: Any | None = None,
) -> None:
    """Write a command to the Firestore queue. Idempotent via update_id as doc ID."""
    from google.cloud.firestore import SERVER_TIMESTAMP

    db = _db or _get_db()
    ref = db.document(f"engagements/{engagement_id}/commands/{update_id}")

    doc: dict[str, Any] = {
        "type": msg_type,
        "payload": payload,
        "receivedAt": SERVER_TIMESTAMP,
        "processedAt": None,
        "status": "pending",
        "telegramChatId": chat_id,
        "telegramMessageId": message_id,
    }
    if snooze_duration is not None:
        doc["snoozeDuration"] = snooze_duration

    ref.set(doc)
    logger.info("queued command %s (type=%s) for engagement %s", update_id, msg_type, engagement_id)
