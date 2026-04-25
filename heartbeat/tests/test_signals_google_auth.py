"""Tests for signals/google_auth.py.

We don't exercise the real google-auth library; we patch the local
imports so the helper's error-mapping logic is exercised in isolation.
"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from heartbeat.signals.google_auth import (
    GOOGLE_SCOPES,
    GoogleAuthFailure,
    load_google_credentials,
)


def test_missing_token_raises_with_typed_error(tmp_path: Path) -> None:
    with pytest.raises(GoogleAuthFailure) as exc_info:
        load_google_credentials(tmp_path / "no-such.json", source="calendar")
    err = exc_info.value.error
    assert err.source == "calendar"
    assert err.error_code == "missing_token"


def test_valid_token_returned_unchanged(tmp_path: Path) -> None:
    token_path = tmp_path / "tok.json"
    token_path.write_text("{}")  # any file body — the SDK call is mocked

    fake_creds = MagicMock()
    fake_creds.valid = True

    with patch(
        "google.oauth2.credentials.Credentials.from_authorized_user_file",
        return_value=fake_creds,
    ):
        creds = load_google_credentials(token_path, source="gmail")
    assert creds is fake_creds


def test_expired_token_refreshes_and_persists(tmp_path: Path) -> None:
    token_path = tmp_path / "tok.json"
    token_path.write_text("old")

    fake_creds = MagicMock()
    fake_creds.valid = False
    fake_creds.refresh_token = "rt"
    fake_creds.to_json.return_value = '{"new": true}'

    with (
        patch(
            "google.oauth2.credentials.Credentials.from_authorized_user_file",
            return_value=fake_creds,
        ),
        patch("google.auth.transport.requests.Request"),
    ):
        creds = load_google_credentials(token_path, source="calendar")
    assert creds is fake_creds
    fake_creds.refresh.assert_called_once()
    # Token file was rewritten with the new content.
    assert token_path.read_text() == '{"new": true}'


def test_refresh_transport_error_maps_to_network_error(tmp_path: Path) -> None:
    from google.auth.exceptions import TransportError

    token_path = tmp_path / "tok.json"
    token_path.write_text("old")

    fake_creds = MagicMock()
    fake_creds.valid = False
    fake_creds.refresh_token = "rt"
    fake_creds.refresh.side_effect = TransportError("DNS down")

    with (
        patch(
            "google.oauth2.credentials.Credentials.from_authorized_user_file",
            return_value=fake_creds,
        ),
        pytest.raises(GoogleAuthFailure) as exc_info,
    ):
        load_google_credentials(token_path, source="calendar")
    assert exc_info.value.error.error_code == "network_error"


def test_refresh_oauth_error_maps_to_oauth_refresh_failed(tmp_path: Path) -> None:
    from google.auth.exceptions import RefreshError

    token_path = tmp_path / "tok.json"
    token_path.write_text("old")

    fake_creds = MagicMock()
    fake_creds.valid = False
    fake_creds.refresh_token = "rt"
    fake_creds.refresh.side_effect = RefreshError("revoked")

    with (
        patch(
            "google.oauth2.credentials.Credentials.from_authorized_user_file",
            return_value=fake_creds,
        ),
        pytest.raises(GoogleAuthFailure) as exc_info,
    ):
        load_google_credentials(token_path, source="gmail")
    err = exc_info.value.error
    assert err.error_code == "oauth_refresh_failed"
    assert err.source == "gmail"


def test_no_refresh_token_returns_terminal_error(tmp_path: Path) -> None:
    token_path = tmp_path / "tok.json"
    token_path.write_text("old")

    fake_creds = MagicMock()
    fake_creds.valid = False
    fake_creds.refresh_token = None  # no refresh token

    with (
        patch(
            "google.oauth2.credentials.Credentials.from_authorized_user_file",
            return_value=fake_creds,
        ),
        pytest.raises(GoogleAuthFailure) as exc_info,
    ):
        load_google_credentials(token_path, source="calendar")
    assert exc_info.value.error.error_code == "oauth_refresh_failed"
    assert "prompt='consent'" in exc_info.value.error.message


def test_token_persist_failure_is_non_fatal(tmp_path: Path) -> None:
    """If we can refresh in memory but can't write the token back, the
    tick still succeeds — next tick will refresh from the older copy."""
    token_path = tmp_path / "tok.json"
    token_path.write_text("old")

    fake_creds = MagicMock()
    fake_creds.valid = False
    fake_creds.refresh_token = "rt"
    fake_creds.to_json.return_value = '{"new": true}'

    with (
        patch(
            "google.oauth2.credentials.Credentials.from_authorized_user_file",
            return_value=fake_creds,
        ),
        patch("google.auth.transport.requests.Request"),
        patch(
            "heartbeat.signals.google_auth.write_atomic",
            side_effect=OSError("disk full"),
        ),
    ):
        creds = load_google_credentials(token_path, source="calendar")
    assert creds is fake_creds
    fake_creds.refresh.assert_called_once()


def test_scope_constants() -> None:
    assert "calendar.readonly" in GOOGLE_SCOPES[0]
    assert "gmail.readonly" in GOOGLE_SCOPES[1]
