"""Shared Google OAuth helper for Calendar + Gmail collectors.

Per pre-code research (Q1):
- The OAuth flow runs **once on the operator's Mac** during install,
  producing a ``token.json`` with a refresh token.
- The Mac install script scp's the token to the VM at
  ``/etc/ikrs-heartbeat/google-token.json``.
- This module loads the token, refreshes it silently when the access
  token expires, and persists the refreshed token back to disk.

Token refresh failures are mapped to typed ``CollectorError`` codes so
the tick orchestrator (E.4) can distinguish:
- ``network_error`` (transient — retry next tick)
- ``oauth_refresh_failed`` (terminal — operator must re-run install
  flow on Mac and scp a new token).
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import TYPE_CHECKING

from heartbeat.signals.base import CollectorError
from heartbeat.signals.state import write_atomic

if TYPE_CHECKING:
    from google.oauth2.credentials import Credentials

logger = logging.getLogger("heartbeat.signals.google_auth")


# Minimum scopes for our collectors. Read-only on both sides — Tier II
# never writes to Calendar or Gmail (the spec is explicit: vault writes
# happen via Tauri, Firestore writes via service account).
GOOGLE_SCOPES = [
    "https://www.googleapis.com/auth/calendar.readonly",
    "https://www.googleapis.com/auth/gmail.readonly",
]


class GoogleAuthFailure(Exception):
    """Raised by ``load_google_credentials`` when auth is unrecoverable.

    Carries a ``CollectorError`` so the caller can fold it directly into
    a ``SignalsBundle.errors`` list.
    """

    def __init__(self, error: CollectorError) -> None:
        super().__init__(error.message)
        self.error = error


def load_google_credentials(
    token_path: Path,
    *,
    source: str = "calendar",
) -> Credentials:
    """Load token.json, refresh if expired, persist back. Returns creds.

    ``source`` is recorded on any ``CollectorError`` so partial failures
    in the bundle name the right collector. Calendar fails first (it's
    the first to call this in the tick), so the default reflects that —
    Gmail callers should override.
    """

    if not token_path.exists():
        raise GoogleAuthFailure(
            CollectorError(
                source=source,  # type: ignore[arg-type]
                error_code="missing_token",
                message=(
                    f"google token not found at {token_path}. Run the "
                    "install script on the operator's Mac to produce "
                    "token.json, then scp it to the VM."
                ),
            )
        )

    # Local imports — lets unit tests bypass google-auth* entirely by
    # patching this function.
    from google.auth.exceptions import RefreshError, TransportError
    from google.auth.transport.requests import Request
    from google.oauth2.credentials import Credentials

    creds: Credentials = Credentials.from_authorized_user_file(  # type: ignore[no-untyped-call]
        str(token_path), GOOGLE_SCOPES
    )

    if creds.valid:
        return creds

    if not creds.refresh_token:
        raise GoogleAuthFailure(
            CollectorError(
                source=source,  # type: ignore[arg-type]
                error_code="oauth_refresh_failed",
                message=(
                    "token has no refresh_token. Re-run install with "
                    "prompt='consent' on the Mac and scp a fresh token.json."
                ),
            )
        )

    try:
        creds.refresh(Request())
    except TransportError as exc:
        # Transient — DNS, connect, TLS. Next tick retries.
        raise GoogleAuthFailure(
            CollectorError(
                source=source,  # type: ignore[arg-type]
                error_code="network_error",
                message=f"google token refresh transport error: {exc}",
            )
        ) from exc
    except RefreshError as exc:
        # Terminal — revoked, expired refresh, scope drift, deleted client.
        raise GoogleAuthFailure(
            CollectorError(
                source=source,  # type: ignore[arg-type]
                error_code="oauth_refresh_failed",
                message=(
                    f"google token refresh failed (likely revoked or "
                    f"scope-drifted): {exc}. Operator must re-run install "
                    "on Mac and scp a new token.json."
                ),
            )
        ) from exc

    # Persist the rotated access token + new expiry. Atomic so a crash
    # mid-write can't corrupt the file.
    try:
        write_atomic(token_path, creds.to_json())  # type: ignore[no-untyped-call]
    except OSError as exc:
        # Non-fatal: we still have valid creds in memory; next tick will
        # try to refresh again from the (older) on-disk copy.
        logger.warning("could not persist refreshed token to %s: %s", token_path, exc)

    return creds
