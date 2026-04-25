"""Per-tick secret bundle.

The dispatch layer needs three things from secrets.env:
- ``FIREBASE_SA_KEY_PATH`` — path to the service-account JSON the Admin
  SDK uses to authenticate. Falls back to ``GOOGLE_APPLICATION_CREDENTIALS``
  for compatibility with Google's standard env var.
- ``TELEGRAM_BOT_TOKEN`` — per-operator bot token from BotFather.
- ``TELEGRAM_CHAT_ID`` — the chat ID (from getUpdates after the operator
  messages the bot once, captured by install.sh).

Each is independently optional. A missing Firebase key disables Firestore
writes; a missing Telegram token disables the push channel. The dispatcher
records "missing" as a typed dispatch error rather than crashing.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class OutputSecrets:
    firestore_credentials_path: Path | None
    telegram_bot_token: str | None
    telegram_chat_id: str | None

    @classmethod
    def from_env(cls) -> OutputSecrets:
        """Build from process env. Used in production after secrets.env
        was loaded by systemd or python-dotenv."""

        sa_key = os.environ.get("FIREBASE_SA_KEY_PATH") or os.environ.get(
            "GOOGLE_APPLICATION_CREDENTIALS"
        )
        return cls(
            firestore_credentials_path=Path(sa_key) if sa_key else None,
            telegram_bot_token=os.environ.get("TELEGRAM_BOT_TOKEN") or None,
            telegram_chat_id=os.environ.get("TELEGRAM_CHAT_ID") or None,
        )
