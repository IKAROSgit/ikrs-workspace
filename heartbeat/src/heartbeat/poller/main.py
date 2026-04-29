"""Bot poller entry point — long-running process for bidirectional Telegram.

Reads getUpdates, validates chat_id allowlist, classifies messages,
writes to Firestore command queue, persists offset, triggers ad-hoc ticks.

The poller NEVER writes to ikrs_tasks, local vault files, or observations.
It is a thin receiver. The tick is the sole writer to everything else.
"""

from __future__ import annotations

import argparse
import logging
import os
import sys
import time
from pathlib import Path
from typing import Any

from heartbeat.config import load_config
from heartbeat.poller.offset import DEFAULT_OFFSET_PATH, read_offset, write_offset
from heartbeat.poller.queue import classify_message, write_command
from heartbeat.poller.telegram import TelegramClient, TelegramError
from heartbeat.poller.trigger import maybe_trigger_tick

logger = logging.getLogger("heartbeat.poller")

_MAX_MESSAGES_PER_MINUTE = 10
_VOICE_MAX_BYTES = 5 * 1024 * 1024  # 5 MB


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="ikrs-heartbeat-poller",
        description="IKAROS Telegram bot poller — bidirectional commands.",
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("/etc/ikrs-heartbeat/heartbeat.toml"),
        help="Path to heartbeat.toml.",
    )
    parser.add_argument(
        "--offset-path",
        type=Path,
        default=DEFAULT_OFFSET_PATH,
        help=f"Path to offset persistence file (default: {DEFAULT_OFFSET_PATH}).",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="DEBUG-level logging.",
    )
    return parser.parse_args(argv)


def _load_allowlist() -> set[int]:
    """Parse TELEGRAM_ALLOWED_CHAT_IDS from env. Empty set = fail-safe drop-all."""
    raw = os.environ.get("TELEGRAM_ALLOWED_CHAT_IDS", "")
    if not raw.strip():
        return set()
    result: set[int] = set()
    for part in raw.split(","):
        part = part.strip()
        if part:
            try:
                result.add(int(part))
            except ValueError:
                logger.warning("invalid chat_id in allowlist: %r", part)
    return result


def _extract_chat_id(update: dict[str, Any]) -> int | None:
    """Extract chat_id from update WITHOUT parsing message body."""
    msg = update.get("message") or update.get("callback_query", {}).get("message")
    if msg and "chat" in msg:
        val = msg["chat"].get("id")
        return int(val) if val is not None else None
    return None


def _extract_message_id(update: dict[str, Any]) -> int:
    msg = update.get("message") or update.get("callback_query", {}).get("message") or {}
    result: int = int(msg.get("message_id", 0))
    return result


def _flush_backlog(client: TelegramClient, offset_path: Path) -> int:
    """First-start backlog flush. Returns the initial offset to use."""
    logger.info("first start: flushing backlog without processing")
    try:
        updates = client.get_updates(offset=-1, limit=1)
    except Exception as exc:
        logger.warning("backlog flush failed: %s; starting from offset 0", exc)
        write_offset(0, offset_path)
        return 0

    if updates:
        last_id: int = int(updates[-1]["update_id"])
        offset: int = last_id + 1
    else:
        offset = 0

    write_offset(offset, offset_path)
    logger.info("backlog flushed; starting from offset %d", offset)
    return offset


def run_poll_loop(
    client: TelegramClient,
    allowlist: set[int],
    engagement_ids: list[str],
    offset_path: Path,
    *,
    _single_pass: bool = False,
    _db: Any | None = None,
) -> None:
    """Main poll loop. Runs forever unless _single_pass=True (for tests)."""

    # Load or initialize offset
    persisted = read_offset(offset_path)
    if persisted is None:
        current_offset = _flush_backlog(client, offset_path)
    else:
        current_offset = persisted
        logger.info("resuming from persisted offset %d", current_offset)

    # Rate limit tracking for chat_id (messages per minute)
    rate_window: dict[int, list[float]] = {}

    while True:
        try:
            updates = client.get_updates(
                offset=current_offset,
                allowed_updates=["message", "callback_query"],
            )
        except TelegramError as exc:
            logger.error("Telegram API error: %s", exc)
            time.sleep(client.get_backoff())
            if _single_pass:
                return
            continue
        except Exception as exc:
            delay = client.get_backoff()
            logger.warning("network error: %s; retrying in %.1fs", exc, delay)
            time.sleep(delay)
            if _single_pass:
                return
            continue

        client.reset_backoff()
        any_queued = False

        for update in updates:
            update_id = update.get("update_id", 0)

            # Step 1-2: Extract and validate chat_id BEFORE body parsing
            chat_id = _extract_chat_id(update)
            if chat_id is None:
                logger.debug("malformed update %d: no chat_id; skipping", update_id)
                current_offset = update_id + 1
                continue

            # Step 3: Fail-safe — empty allowlist drops all
            if not allowlist:
                logger.warning(
                    "allowlist empty; dropping update %d from chat %d",
                    update_id, chat_id,
                )
                current_offset = update_id + 1
                continue

            if chat_id not in allowlist:
                logger.info("chat_id %d not in allowlist; dropping update %d", chat_id, update_id)
                current_offset = update_id + 1
                continue

            # Rate limit per chat_id
            now = time.time()
            window = rate_window.setdefault(chat_id, [])
            window[:] = [t for t in window if now - t < 60]
            if len(window) >= _MAX_MESSAGES_PER_MINUTE:
                logger.info("rate limited chat_id %d; dropping update %d", chat_id, update_id)
                client.send_message(chat_id, "Slow down — max 10 messages per minute.")
                current_offset = update_id + 1
                continue
            window.append(now)

            # Step 4: Extract message
            message = update.get("message")
            if not message:
                logger.debug("update %d has no message field; skipping", update_id)
                current_offset = update_id + 1
                continue

            # Step 5: Classify
            msg_type, payload, snooze_duration = classify_message(message)

            # Step 6: Voice size check (before download — G.3 does actual STT)
            if msg_type == "voice":
                voice = message.get("voice", {})
                file_size = voice.get("file_size", 0)
                if file_size > _VOICE_MAX_BYTES:
                    client.send_message(
                        chat_id,
                        "Voice too long; keep under 30 seconds or type.",
                    )
                    current_offset = update_id + 1
                    continue

            # Step 7: Write to command queue
            # Use first engagement for now (G.2 v1 = single-engagement poller)
            eid = engagement_ids[0] if engagement_ids else ""
            if not eid:
                logger.error("no engagement_id configured; cannot queue command")
                current_offset = update_id + 1
                continue

            message_id = _extract_message_id(update)
            try:
                write_command(
                    engagement_id=eid,
                    update_id=update_id,
                    msg_type=msg_type,
                    payload=payload,
                    chat_id=chat_id,
                    message_id=message_id,
                    snooze_duration=snooze_duration,
                    _db=_db,
                )
                any_queued = True
            except Exception as exc:
                logger.error(
                    "queue write failed for update %d: %s; NOT advancing offset",
                    update_id, exc,
                )
                # Do NOT advance offset — Telegram will re-deliver
                continue

            # Step 8: Advance offset
            current_offset = update_id + 1

        # Persist offset after batch
        if updates:
            write_offset(current_offset, offset_path)

        # Trigger ad-hoc tick if anything was queued
        if any_queued:
            maybe_trigger_tick()

        if _single_pass:
            return


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )

    try:
        config = load_config(args.config)
    except (FileNotFoundError, ValueError) as exc:
        logger.error("config error: %s", exc)
        return 2

    token = os.environ.get("TELEGRAM_BOT_TOKEN", "")
    if not token:
        logger.error("TELEGRAM_BOT_TOKEN not set")
        return 2

    allowlist = _load_allowlist()
    if not allowlist:
        logger.warning(
            "TELEGRAM_ALLOWED_CHAT_IDS is empty — all messages "
            "will be dropped (fail-safe)"
        )

    engagement_ids = [e.id for e in config.engagements]
    if not engagement_ids:
        logger.error("no engagements configured")
        return 2

    client = TelegramClient(token)
    logger.info(
        "starting poller: %d engagement(s), allowlist=%s",
        len(engagement_ids),
        allowlist or "(empty — fail-safe)",
    )

    run_poll_loop(
        client=client,
        allowlist=allowlist,
        engagement_ids=engagement_ids,
        offset_path=args.offset_path,
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
