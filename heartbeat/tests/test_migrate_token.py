"""Tests for scripts/migrate-token-to-firestore.py.

Covers all exit codes and the idempotency / verification logic.
Firebase Admin SDK is mocked — no real Firestore calls.
"""

from __future__ import annotations

import base64
import importlib.util
import json
import os
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

_SCRIPT_PATH = Path(__file__).parent.parent / "scripts" / "migrate-token-to-firestore.py"
_spec = importlib.util.spec_from_file_location("migrate_script", _SCRIPT_PATH)
assert _spec is not None and _spec.loader is not None
migrate_script = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(migrate_script)


def _write_legacy_token(path: Path) -> dict[str, Any]:
    """Write a realistic legacy google-token.json and return its content."""
    token = {
        "token": "ya29.access-test",
        "refresh_token": "1//refresh-test",
        "token_uri": "https://oauth2.googleapis.com/token",
        "client_id": "test-client-id.apps.googleusercontent.com",
        "client_secret": "test-client-secret",
        "scopes": [
            "https://www.googleapis.com/auth/calendar.readonly",
            "https://www.googleapis.com/auth/gmail.readonly",
        ],
        "universe_domain": "googleapis.com",
        "account": "",
        "expiry": "2026-04-28T12:00:00.000000Z",
    }
    path.write_text(json.dumps(token))
    return token


def _make_key() -> tuple[str, bytes]:
    """Generate a test encryption key. Returns (base64_str, raw_bytes)."""
    raw = os.urandom(32)
    return base64.b64encode(raw).decode("ascii"), raw


def _make_env(key_b64: str) -> dict[str, str]:
    """Build env dict with encryption key + SA path stub."""
    return {
        "TOKEN_ENCRYPTION_KEY": key_b64,
        "TOKEN_ENCRYPTION_KEY_VERSION": "1",
        "FIREBASE_SA_KEY_PATH": "/dev/null",
    }


def _mock_firestore(existing_doc: dict[str, Any] | None = None) -> MagicMock:
    """Create a mock Firestore client. If existing_doc is given, .get() returns it."""
    mock_db = MagicMock()
    mock_ref = MagicMock()
    mock_db.document.return_value = mock_ref

    snap = MagicMock()
    if existing_doc is not None:
        snap.exists = True
        snap.to_dict.return_value = existing_doc
    else:
        snap.exists = False
        snap.to_dict.return_value = {}

    mock_ref.get.return_value = snap
    return mock_db


class TestMigrateTokenExitCodes:
    """Test all documented exit codes."""

    def test_legacy_file_missing_exits_1(self, tmp_path: Path) -> None:
        rc = migrate_script.main([
            "eng-123",
            "--token-path", str(tmp_path / "nonexistent.json"),
        ])
        assert rc == 1

    def test_encryption_key_missing_exits_2(self, tmp_path: Path) -> None:
        token_path = tmp_path / "token.json"
        _write_legacy_token(token_path)
        with patch.dict(os.environ, {}, clear=True):
            # Ensure TOKEN_ENCRYPTION_KEY is absent
            os.environ.pop("TOKEN_ENCRYPTION_KEY", None)
            rc = migrate_script.main([
                "eng-123",
                "--token-path", str(token_path),
            ])
        assert rc == 2

    @patch("firebase_admin._apps", {"[DEFAULT]": MagicMock()})
    def test_firestore_write_failure_exits_3(self, tmp_path: Path) -> None:
        token_path = tmp_path / "token.json"
        _write_legacy_token(token_path)
        key_b64, _ = _make_key()

        mock_db = _mock_firestore(existing_doc=None)
        mock_db.document.return_value.set.side_effect = Exception("permission denied")

        with (
            patch.dict(os.environ, _make_env(key_b64)),
            patch("firebase_admin.firestore.client", return_value=mock_db),
        ):
            rc = migrate_script.main([
                "eng-123",
                "--token-path", str(token_path),
            ])
        assert rc == 3

    @patch("firebase_admin._apps", {"[DEFAULT]": MagicMock()})
    def test_verification_failure_exits_4(self, tmp_path: Path) -> None:
        token_path = tmp_path / "token.json"
        _write_legacy_token(token_path)
        key_b64, _ = _make_key()

        mock_db = _mock_firestore(existing_doc=None)
        # After write, read-back returns a doc that doesn't exist
        call_count = [0]
        original_snap_no_doc = MagicMock()
        original_snap_no_doc.exists = False
        original_snap_no_doc.to_dict.return_value = {}

        def get_side_effect() -> Any:
            call_count[0] += 1
            if call_count[0] == 1:
                # First call: check if already migrated → not found
                return original_snap_no_doc
            # Second call: verification read-back → also not found
            return original_snap_no_doc

        mock_db.document.return_value.get.side_effect = get_side_effect

        with (
            patch.dict(os.environ, _make_env(key_b64)),
            patch("firebase_admin.firestore.client", return_value=mock_db),
        ):
            rc = migrate_script.main([
                "eng-123",
                "--token-path", str(token_path),
            ])
        assert rc == 4

    @patch("firebase_admin._apps", {"[DEFAULT]": MagicMock()})
    def test_success_path_exits_0(self, tmp_path: Path) -> None:
        token_path = tmp_path / "token.json"
        _write_legacy_token(token_path)
        key_b64, key_bytes = _make_key()

        mock_db = _mock_firestore(existing_doc=None)

        # Capture what gets written so we can return it on read-back
        written_data: dict[str, Any] = {}

        def capture_set(data: dict[str, Any]) -> None:
            written_data.update(data)

        mock_db.document.return_value.set.side_effect = capture_set

        call_count = [0]

        def get_side_effect() -> Any:
            call_count[0] += 1
            if call_count[0] == 1:
                # First: idempotency check → not found
                snap = MagicMock()
                snap.exists = False
                snap.to_dict.return_value = {}
                return snap
            # Second: verification read-back → return what was written
            snap = MagicMock()
            snap.exists = True
            snap.to_dict.return_value = dict(written_data)
            return snap

        mock_db.document.return_value.get.side_effect = get_side_effect

        with (
            patch.dict(os.environ, _make_env(key_b64)),
            patch("firebase_admin.firestore.client", return_value=mock_db),
        ):
            rc = migrate_script.main([
                "eng-123",
                "--token-path", str(token_path),
            ])
        assert rc == 0
        # Legacy file must still exist
        assert token_path.exists()

    @patch("firebase_admin._apps", {"[DEFAULT]": MagicMock()})
    def test_dry_run_does_no_writes(self, tmp_path: Path) -> None:
        token_path = tmp_path / "token.json"
        _write_legacy_token(token_path)
        key_b64, _ = _make_key()

        mock_db = _mock_firestore(existing_doc=None)

        with (
            patch.dict(os.environ, _make_env(key_b64)),
            patch("firebase_admin.firestore.client", return_value=mock_db),
        ):
            rc = migrate_script.main([
                "eng-123",
                "--token-path", str(token_path),
                "--dry-run",
            ])
        assert rc == 0
        mock_db.document.return_value.set.assert_not_called()

    @patch("firebase_admin._apps", {"[DEFAULT]": MagicMock()})
    def test_idempotent_rerun_exits_0(self, tmp_path: Path) -> None:
        token_path = tmp_path / "token.json"
        legacy = _write_legacy_token(token_path)
        key_b64, key_bytes = _make_key()

        # Build an already-migrated doc
        translated = migrate_script._translate_legacy_token(legacy)
        plaintext = json.dumps(translated, separators=(",", ":"), sort_keys=True)
        ct_b64, iv_b64 = migrate_script._encrypt(plaintext, key_bytes)

        existing_doc = {
            "ciphertext": ct_b64,
            "iv": iv_b64,
            "keyVersion": 1,
            "writtenBy": "migration",
        }
        mock_db = _mock_firestore(existing_doc=existing_doc)

        with (
            patch.dict(os.environ, _make_env(key_b64)),
            patch("firebase_admin.firestore.client", return_value=mock_db),
        ):
            rc = migrate_script.main([
                "eng-123",
                "--token-path", str(token_path),
            ])
        assert rc == 0
        # No write should have been attempted
        mock_db.document.return_value.set.assert_not_called()
