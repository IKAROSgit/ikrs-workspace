"""Tests for the Phase G bot poller (G.2).

12 tests covering the spec's test plan. All mock Telegram API + Firestore.
"""

from __future__ import annotations

import os
import time
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

from heartbeat.poller.main import (
    _load_allowlist,
    run_poll_loop,
)
from heartbeat.poller.offset import read_offset, write_offset
from heartbeat.poller.queue import classify_message
from heartbeat.poller.trigger import maybe_trigger_tick


def _make_update(
    update_id: int,
    chat_id: int = 123,
    text: str = "hello",
    voice: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build a minimal Telegram update dict."""
    msg: dict[str, Any] = {
        "message_id": update_id * 10,
        "chat": {"id": chat_id},
        "date": int(time.time()),
    }
    if voice is not None:
        msg["voice"] = voice
    else:
        msg["text"] = text
    return {"update_id": update_id, "message": msg}


def _mock_client(updates: list[dict[str, Any]]) -> MagicMock:
    """Create a TelegramClient mock that returns `updates` once then empty."""
    client = MagicMock()
    client.get_updates.side_effect = [updates, []]
    client.reset_backoff = MagicMock()
    client.get_backoff.return_value = 0.01
    return client


def _mock_db() -> MagicMock:
    db = MagicMock()
    return db


# ---------------------------------------------------------------------------
# Test 1: Text message → queue
# ---------------------------------------------------------------------------

class TestTextMessageToQueue:
    def test_text_message_queued(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(100, offset_path)

        update = _make_update(101, chat_id=555, text="/ask what is the BLR status?")
        client = _mock_client([update])
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist={555},
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        # Verify queue write
        db.document.assert_called_with("engagements/eng-1/commands/101")
        set_call = db.document.return_value.set
        set_call.assert_called_once()
        doc = set_call.call_args[0][0]
        assert doc["type"] == "text"
        assert doc["payload"] == "what is the BLR status?"
        assert doc["status"] == "pending"
        assert doc["telegramChatId"] == 555

        # Offset advanced
        assert read_offset(offset_path) == 102


# ---------------------------------------------------------------------------
# Test 2: Voice message → size check → queue
# ---------------------------------------------------------------------------

class TestVoiceMessageToQueue:
    def test_voice_message_under_limit_queued(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(200, offset_path)

        voice = {"file_id": "abc123", "file_size": 100_000, "duration": 5}
        update = _make_update(201, chat_id=555, voice=voice)
        client = _mock_client([update])
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist={555},
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        set_call = db.document.return_value.set
        set_call.assert_called_once()
        doc = set_call.call_args[0][0]
        assert doc["type"] == "voice"
        assert doc["payload"] == "abc123"


# ---------------------------------------------------------------------------
# Test 3: Malformed update → reject, offset advances
# ---------------------------------------------------------------------------

class TestMalformedUpdate:
    def test_no_message_field_skipped(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(300, offset_path)

        update = {"update_id": 301}  # no message field
        client = _mock_client([update])
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist={555},
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        # No queue write
        db.document.return_value.set.assert_not_called()
        # Offset still advances (malformed update consumed)
        assert read_offset(offset_path) == 302


# ---------------------------------------------------------------------------
# Test 4: chat_id not in allowlist → drop
# ---------------------------------------------------------------------------

class TestChatIdNotInAllowlist:
    def test_non_allowed_chat_id_dropped(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(400, offset_path)

        update = _make_update(401, chat_id=999, text="hello")
        client = _mock_client([update])
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist={555},  # 999 not in list
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        db.document.return_value.set.assert_not_called()
        assert read_offset(offset_path) == 402


# ---------------------------------------------------------------------------
# Test 5: Allowlist empty → drop everything (fail-safe)
# ---------------------------------------------------------------------------

class TestEmptyAllowlistDropsAll:
    def test_empty_allowlist_drops_everything(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(500, offset_path)

        update = _make_update(501, chat_id=555, text="hello")
        client = _mock_client([update])
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist=set(),  # empty = fail-safe
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        db.document.return_value.set.assert_not_called()


# ---------------------------------------------------------------------------
# Test 6: Existing offset file → resume
# ---------------------------------------------------------------------------

class TestResumeFromOffset:
    def test_existing_offset_resumes(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(600, offset_path)

        client = _mock_client([])  # no updates
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist={555},
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        # getUpdates called with persisted offset
        client.get_updates.assert_called()
        first_call = client.get_updates.call_args_list[0]
        assert first_call.kwargs.get("offset") == 600 or first_call[1].get("offset") == 600


# ---------------------------------------------------------------------------
# Test 7: No offset file → first-start backlog flush
# ---------------------------------------------------------------------------

class TestFirstStartFlush:
    def test_no_offset_file_flushes_backlog(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        # No file exists

        # Mock client: first call (flush) returns one update, second (poll) returns empty
        client = MagicMock()
        client.get_updates.side_effect = [
            [{"update_id": 999}],  # flush call with offset=-1
            [],  # first real poll
        ]
        client.reset_backoff = MagicMock()
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client,
                allowlist={555},
                engagement_ids=["eng-1"],
                offset_path=offset_path,
                _single_pass=True,
                _db=db,
            )

        # First getUpdates should have offset=-1 (flush)
        first_call = client.get_updates.call_args_list[0]
        assert first_call[1].get("offset") == -1 or first_call.kwargs.get("offset") == -1

        # Offset should be 1000 (999 + 1)
        assert read_offset(offset_path) == 1000

        # No commands queued (backlog was flushed, not processed)
        db.document.return_value.set.assert_not_called()


# ---------------------------------------------------------------------------
# Test 8: Queue write succeeds + offset persist simulated failure → idempotent re-queue
# ---------------------------------------------------------------------------

class TestCrashRecoveryQueueSucceeds:
    def test_redelivery_is_idempotent(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(800, offset_path)

        update = _make_update(801, chat_id=555, text="hello")
        db = _mock_db()

        # First delivery: queue write succeeds
        client1 = _mock_client([update])
        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client1, allowlist={555}, engagement_ids=["eng-1"],
                offset_path=offset_path, _single_pass=True, _db=db,
            )
        assert db.document.return_value.set.call_count == 1

        # Simulate: offset persisted to 802. Now re-deliver same update
        # (as if offset persist failed and Telegram re-sends).
        write_offset(800, offset_path)  # reset offset
        client2 = _mock_client([update])
        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client2, allowlist={555}, engagement_ids=["eng-1"],
                offset_path=offset_path, _single_pass=True, _db=db,
            )

        # set() called again with same doc ID — idempotent overwrite
        assert db.document.return_value.set.call_count == 2
        # Both calls used the same doc path
        doc_calls = db.document.call_args_list
        assert "commands/801" in str(doc_calls[-1])


# ---------------------------------------------------------------------------
# Test 9: Queue write fails → offset not bumped
# ---------------------------------------------------------------------------

class TestCrashRecoveryQueueFails:
    def test_queue_write_failure_preserves_offset(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(900, offset_path)

        update = _make_update(901, chat_id=555, text="hello")
        client = _mock_client([update])
        db = _mock_db()
        db.document.return_value.set.side_effect = Exception("Firestore down")

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client, allowlist={555}, engagement_ids=["eng-1"],
                offset_path=offset_path, _single_pass=True, _db=db,
            )

        # Offset should NOT have advanced past 901
        assert read_offset(offset_path) == 900


# ---------------------------------------------------------------------------
# Test 10: Rate limit — 5 commands in 1s → 1 trigger, 4 skipped
# ---------------------------------------------------------------------------

class TestRateLimit:
    @patch("heartbeat.poller.trigger.subprocess.run")
    def test_rate_limit_trigger(self, mock_run: MagicMock, tmp_path: Path) -> None:
        mock_run.return_value = MagicMock(returncode=0)
        ts_path = tmp_path / "last-trigger.timestamp"

        # First call: should trigger
        assert maybe_trigger_tick(ts_path, _now=1000.0) is True
        assert mock_run.call_count == 1

        # Next 4 within 10s: should be skipped (rate-limited)
        for i in range(4):
            assert maybe_trigger_tick(ts_path, _now=1001.0 + i) is False
        assert mock_run.call_count == 1  # no additional calls

        # After 10s: should trigger again
        assert maybe_trigger_tick(ts_path, _now=1011.0) is True
        assert mock_run.call_count == 2


# ---------------------------------------------------------------------------
# Test 11: Network error → no crash (exponential backoff)
# ---------------------------------------------------------------------------

class TestNetworkError:
    def test_network_error_uses_backoff(self, tmp_path: Path) -> None:
        """On network error, poller calls get_backoff for exponential delay
        and does NOT crash. With _single_pass, it exits after the error."""
        import requests

        offset_path = tmp_path / "offset"
        write_offset(1100, offset_path)

        client = MagicMock()
        client.get_updates.side_effect = requests.ConnectionError("DNS failed")
        client.get_backoff.return_value = 0.01
        client.reset_backoff = MagicMock()
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            # _single_pass exits after the first error
            run_poll_loop(
                client=client, allowlist={555}, engagement_ids=["eng-1"],
                offset_path=offset_path, _single_pass=True, _db=db,
            )

        # Backoff was requested (exponential delay mechanism invoked)
        client.get_backoff.assert_called_once()
        # No crash, no queue writes
        db.document.return_value.set.assert_not_called()


# ---------------------------------------------------------------------------
# Test 12: Voice message >5MB → rejected, no crash
# ---------------------------------------------------------------------------

class TestOversizedVoice:
    def test_oversized_voice_rejected(self, tmp_path: Path) -> None:
        offset_path = tmp_path / "offset"
        write_offset(1200, offset_path)

        voice = {"file_id": "big", "file_size": 10_000_000, "duration": 60}
        update = _make_update(1201, chat_id=555, voice=voice)
        client = _mock_client([update])
        db = _mock_db()

        with patch("heartbeat.poller.main.maybe_trigger_tick"):
            run_poll_loop(
                client=client, allowlist={555}, engagement_ids=["eng-1"],
                offset_path=offset_path, _single_pass=True, _db=db,
            )

        # No queue write
        db.document.return_value.set.assert_not_called()
        # Reply sent to operator
        client.send_message.assert_called_once()
        assert "voice" in client.send_message.call_args[0][1].lower()
        # Offset still advances (update consumed)
        assert read_offset(offset_path) == 1202


# ---------------------------------------------------------------------------
# Extra: classify_message unit tests
# ---------------------------------------------------------------------------

class TestClassifyMessage:
    def test_confirm(self) -> None:
        assert classify_message({"text": "/confirm abc123"}) == ("confirm", "abc123", None)

    def test_snooze(self) -> None:
        assert classify_message({"text": "/snooze abc123 2h"}) == ("snooze", "abc123", "2h")

    def test_dismiss(self) -> None:
        assert classify_message({"text": "/dismiss abc123"}) == ("dismiss", "abc123", None)

    def test_ask(self) -> None:
        result = classify_message({"text": "/ask what time is it?"})
        assert result == ("text", "what time is it?", None)

    def test_plain_text(self) -> None:
        assert classify_message({"text": "hello world"}) == ("text", "hello world", None)

    def test_voice(self) -> None:
        assert classify_message({"voice": {"file_id": "xyz"}}) == ("voice", "xyz", None)

    def test_load_allowlist(self) -> None:
        with patch.dict(os.environ, {"TELEGRAM_ALLOWED_CHAT_IDS": "123,456,789"}):
            result = _load_allowlist()
        assert result == {123, 456, 789}

    def test_load_allowlist_empty(self) -> None:
        with patch.dict(os.environ, {"TELEGRAM_ALLOWED_CHAT_IDS": ""}):
            result = _load_allowlist()
        assert result == set()
