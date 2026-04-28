"""Regression tests for Phase F.7 adversarial challenge fixes.

Each test targets a specific bug found by the post-code challenge agent.
If any of these regress, the corresponding showstopper/block is back.
"""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, call, patch

import pytest

from heartbeat.signals.base import CollectorError
from heartbeat.tick import _ERROR_PRECEDENCE, _pick_error_code


# ---------------------------------------------------------------------------
# F.7 fix #1: _get_db() must use a NAMED firebase app, not the default.
# Bug: fs.client() with no app= arg requires the default app, which the
# heartbeat never initializes. The outputs layer uses a named app.
# ---------------------------------------------------------------------------


class TestGetDbUsesNamedApp:
    """Assert _get_db() creates a named app, not the default."""

    def test_get_db_initializes_named_app(self) -> None:
        """The app name must be 'heartbeat-token-sync', NOT the default."""
        import heartbeat.signals.firestore_tokens as ft

        mock_app = MagicMock()
        mock_client = MagicMock()

        with (
            patch.object(ft, "_FS_CLIENT", None),
            patch.dict(os.environ, {"FIREBASE_SA_KEY_PATH": "/fake/sa.json"}),
            patch("firebase_admin.get_app", side_effect=ValueError("not found")),
            patch("firebase_admin.credentials.Certificate", return_value=MagicMock()),
            patch("firebase_admin.initialize_app", return_value=mock_app) as mock_init,
            patch("firebase_admin.firestore.client", return_value=mock_client) as mock_fs_client,
        ):
            result = ft._get_db()

        # Must call initialize_app with name= (not the default)
        mock_init.assert_called_once()
        init_call = mock_init.call_args
        assert init_call.kwargs.get("name") == "heartbeat-token-sync"

        # Must pass app= to firestore.client()
        mock_fs_client.assert_called_once()
        fs_call = mock_fs_client.call_args
        assert "app" in fs_call.kwargs

        assert result is mock_client

        # Clean up module-level cache
        ft._FS_CLIENT = None


# ---------------------------------------------------------------------------
# F.7 fix #2: _payload_to_credentials must NOT pass scopes= to Credentials.
# Bug: Tauri grants gmail.modify + calendar.events but heartbeat declared
# calendar.readonly + gmail.readonly, causing scope-change errors.
# ---------------------------------------------------------------------------


class TestNoScopesOnCredentials:
    """Assert Credentials() is called without scopes= argument."""

    def test_payload_to_credentials_omits_scopes(self) -> None:
        from heartbeat.signals.firestore_tokens import TokenPayload, _payload_to_credentials

        payload = TokenPayload(
            access_token="ya29.test",
            refresh_token="1//test",
            expires_at=9999999999,
            client_id="cid",
            client_secret="csec",
        )

        with patch(
            "google.oauth2.credentials.Credentials"
        ) as MockCreds:
            MockCreds.return_value = MagicMock()
            _payload_to_credentials(payload)

            MockCreds.assert_called_once()
            call_kwargs = MockCreds.call_args.kwargs
            # scopes must NOT be in the kwargs
            assert "scopes" not in call_kwargs, (
                "scopes= was passed to Credentials(); this causes scope-change "
                "errors when Tauri grants broader scopes than heartbeat declares"
            )


# ---------------------------------------------------------------------------
# F.7 fix #5: _ERROR_PRECEDENCE must include Phase F error codes at 85,
# and they must outrank network_error (40).
# ---------------------------------------------------------------------------


class TestErrorPrecedencePhaseF:
    """Assert Phase F error codes are ranked correctly in telemetry."""

    def test_key_version_unknown_in_precedence(self) -> None:
        assert "key_version_unknown" in _ERROR_PRECEDENCE
        assert _ERROR_PRECEDENCE["key_version_unknown"] == 85

    def test_token_decrypt_failed_in_precedence(self) -> None:
        assert "token_decrypt_failed" in _ERROR_PRECEDENCE
        assert _ERROR_PRECEDENCE["token_decrypt_failed"] == 85

    def test_phase_f_codes_outrank_network_error(self) -> None:
        """If both a network error and a key version error fire on the same
        tick, telemetry must surface the key version error (operator action
        required), not the transient network error."""
        errors = [
            CollectorError(
                source="calendar",
                error_code="network_error",
                message="DNS failed",
            ),
            CollectorError(
                source="gmail",
                error_code="key_version_unknown",
                message="key version 2 not found",
            ),
        ]
        winner = _pick_error_code(errors)
        assert winner == "key_version_unknown"

    def test_decrypt_failed_outranks_vault_io(self) -> None:
        errors = [
            CollectorError(
                source="vault",
                error_code="vault_io_error",
                message="permission denied",
            ),
            CollectorError(
                source="calendar",
                error_code="token_decrypt_failed",
                message="bad key",
            ),
        ]
        winner = _pick_error_code(errors)
        assert winner == "token_decrypt_failed"


# ---------------------------------------------------------------------------
# F.7 fix #6: install.sh must generate [[engagements]] format, not legacy.
# ---------------------------------------------------------------------------


class TestInstallShGeneratesEngagementsFormat:
    """Verify install.sh produces [[engagements]] TOML, not flat format."""

    def test_install_script_syntax_valid(self) -> None:
        """bash -n catches syntax errors in install.sh."""
        import subprocess

        script = Path(__file__).parent.parent / "scripts" / "install.sh"
        result = subprocess.run(
            ["bash", "-n", str(script)],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, f"bash -n failed: {result.stderr}"

    def test_install_template_has_engagements_array(self) -> None:
        """The heredoc in install.sh must contain [[engagements]], not
        flat engagement_id at top level."""
        script = Path(__file__).parent.parent / "scripts" / "install.sh"
        content = script.read_text()
        # The TOML template in the heredoc
        assert "[[engagements]]" in content, (
            "install.sh must generate [[engagements]] format, not legacy flat format"
        )
        # Should NOT have bare engagement_id = at top level in the template
        # (the heredoc is between 'cat > "$ETC_DIR/heartbeat.toml" <<EOF' and 'EOF')
        import re

        heredoc_match = re.search(
            r'cat > "\$ETC_DIR/heartbeat\.toml" <<EOF\n(.*?)\nEOF',
            content,
            re.DOTALL,
        )
        assert heredoc_match is not None, "Could not find heartbeat.toml heredoc"
        heredoc = heredoc_match.group(1)
        # engagement_id = should NOT appear as a top-level key
        lines_before_engagements = heredoc.split("[[engagements]]")[0]
        assert "engagement_id" not in lines_before_engagements, (
            "install.sh heredoc has flat engagement_id before [[engagements]] — "
            "this generates the deprecated legacy format"
        )


# ---------------------------------------------------------------------------
# F.7 fix #9 (WARN): encryption key persisted before display.
# ---------------------------------------------------------------------------


class TestInstallKeyPersistOrder:
    """The key must be written to secrets.env BEFORE the backup prompt."""

    def test_key_persist_before_prompt(self) -> None:
        """In install.sh, the append to SECRETS_FILE must come before
        the 'Press ENTER' read."""
        script = Path(__file__).parent.parent / "scripts" / "install.sh"
        content = script.read_text()
        # Find positions of the early persist and the prompt
        persist_pos = content.find('>> "$SECRETS_FILE"')
        prompt_pos = content.find("Press ENTER to acknowledge")
        assert persist_pos > 0, "Could not find early key persist in install.sh"
        assert prompt_pos > 0, "Could not find backup prompt in install.sh"
        assert persist_pos < prompt_pos, (
            "install.sh writes key to secrets.env AFTER the backup prompt — "
            "if the operator Ctrl+C's during the prompt, the key is lost"
        )
