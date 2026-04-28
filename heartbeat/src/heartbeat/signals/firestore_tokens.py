"""Phase F — Read encrypted OAuth tokens from Firestore.

Replaces the Phase E pattern of reading a static google-token.json.
Tokens are written by the Tauri app (AES-256-GCM encrypted) to
``engagements/{eid}/google_tokens/google`` and read + decrypted here.

If the access token is expired, this module refreshes it via the
Google token endpoint and writes the updated encrypted payload back
to Firestore (optimistic-concurrency: re-read doc after refresh,
skip writeback if another writer updated it in between).

Falls back to the legacy file-based path if Firestore token is
unavailable (backwards-compatible with Phase E deployments).
"""

from __future__ import annotations

import base64
import json
import logging
import os
import time
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from heartbeat.signals.base import CollectorError
from heartbeat.signals.google_auth import (
    GoogleAuthFailure,
    GoogleSource,
    load_google_credentials,
)

if TYPE_CHECKING:
    from pathlib import Path

    from google.oauth2.credentials import Credentials

logger = logging.getLogger("heartbeat.signals.firestore_tokens")

# Cached Firestore client, initialized lazily via _get_db().
# typed as Any because google-cloud-firestore is untyped.
_FS_CLIENT: Any | None = None


def _get_db() -> Any:
    """Return a Firestore client, initializing the Firebase app if needed.

    Uses the same SA path as the outputs layer (FIREBASE_SA_KEY_PATH env var)
    but with a dedicated named app to avoid collisions.
    """
    global _FS_CLIENT  # noqa: PLW0603
    if _FS_CLIENT is not None:
        return _FS_CLIENT

    import firebase_admin
    from firebase_admin import credentials
    from firebase_admin import firestore as fs

    sa_path = os.environ.get("FIREBASE_SA_KEY_PATH", "/etc/ikrs-heartbeat/firebase-sa.json")
    app_name = "heartbeat-token-sync"
    try:
        app = firebase_admin.get_app(app_name)  # type: ignore[no-untyped-call]
    except ValueError:
        cred = credentials.Certificate(sa_path)  # type: ignore[no-untyped-call]
        app = firebase_admin.initialize_app(cred, name=app_name)  # type: ignore[no-untyped-call]

    _FS_CLIENT = fs.client(app=app)
    return _FS_CLIENT


@dataclass(frozen=True)
class TokenPayload:
    """Mirrors the Tauri TokenPayload struct."""

    access_token: str
    refresh_token: str
    expires_at: int
    client_id: str
    client_secret: str


def _get_encryption_key() -> tuple[bytes, int] | None:
    """Read the base64-encoded AES key + version from env.

    Returns (key_bytes, version) or None if not configured.
    """
    key_b64 = os.environ.get("TOKEN_ENCRYPTION_KEY", "")
    if not key_b64:
        return None
    try:
        key_bytes = base64.b64decode(key_b64)
    except Exception:
        logger.warning("TOKEN_ENCRYPTION_KEY is not valid base64")
        return None
    if len(key_bytes) != 32:
        logger.warning(
            "TOKEN_ENCRYPTION_KEY decoded to %d bytes, expected 32", len(key_bytes)
        )
        return None
    version = int(os.environ.get("TOKEN_ENCRYPTION_KEY_VERSION", "1"))
    return key_bytes, version


def _get_prev_encryption_key() -> tuple[bytes, int] | None:
    """Read the previous key for rotation fallback."""
    key_b64 = os.environ.get("TOKEN_ENCRYPTION_KEY_PREV", "")
    if not key_b64:
        return None
    try:
        key_bytes = base64.b64decode(key_b64)
    except Exception:
        return None
    if len(key_bytes) != 32:
        return None
    version = int(os.environ.get("TOKEN_ENCRYPTION_KEY_PREV_VERSION", "0"))
    return key_bytes, version


def _decrypt(ciphertext_b64: str, iv_b64: str, key_bytes: bytes) -> str:
    """Decrypt AES-256-GCM ciphertext. Returns plaintext string.

    The ciphertext_b64 contains ciphertext || 16-byte GCM auth tag
    (concatenated), matching WebCrypto's default output.
    """
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM

    ct = base64.b64decode(ciphertext_b64)
    iv = base64.b64decode(iv_b64)
    aesgcm = AESGCM(key_bytes)
    plaintext = aesgcm.decrypt(iv, ct, None)
    return plaintext.decode("utf-8")


def _encrypt(plaintext: str, key_bytes: bytes) -> tuple[str, str]:
    """Encrypt with AES-256-GCM. Returns (ciphertext_b64, iv_b64).

    ciphertext_b64 contains ciphertext || 16-byte GCM auth tag.
    """
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM

    iv = os.urandom(12)
    aesgcm = AESGCM(key_bytes)
    ct = aesgcm.encrypt(iv, plaintext.encode("utf-8"), None)
    return base64.b64encode(ct).decode("ascii"), base64.b64encode(iv).decode("ascii")


def _read_firestore_token(
    engagement_id: str,
    source: GoogleSource,
) -> tuple[TokenPayload, dict[str, object]] | None:
    """Read and decrypt token from Firestore.

    Returns (payload, raw_doc_dict) or None if unavailable.
    Raises GoogleAuthFailure on decrypt/key errors.
    """
    key_info = _get_encryption_key()
    if key_info is None:
        return None  # Key not configured — fall back to legacy

    key_bytes, key_version = key_info

    db = _get_db()
    ref = db.document(f"engagements/{engagement_id}/google_tokens/google")
    snap = ref.get()

    if not snap.exists:
        return None  # No Firestore token — fall back to legacy

    doc = snap.to_dict() or {}
    ct_b64 = doc.get("ciphertext", "")
    iv_b64 = doc.get("iv", "")
    doc_key_version = doc.get("keyVersion", 1)

    if not ct_b64 or not iv_b64:
        logger.warning("Firestore token doc exists but is empty for engagement %s", engagement_id)
        return None

    # Try decryption with the matching key version
    decrypt_key: bytes | None = None
    if doc_key_version == key_version:
        decrypt_key = key_bytes
    else:
        prev = _get_prev_encryption_key()
        if prev is not None and doc_key_version == prev[1]:
            decrypt_key = prev[0]

    if decrypt_key is None:
        raise GoogleAuthFailure(
            CollectorError(
                source=source,
                error_code="key_version_unknown",
                message=(
                    f"Firestore token has keyVersion={doc_key_version} but "
                    f"local key is version {key_version}. Update the "
                    "encryption key on this machine."
                ),
            )
        )

    try:
        plaintext = _decrypt(ct_b64, iv_b64, decrypt_key)
    except Exception as exc:
        raise GoogleAuthFailure(
            CollectorError(
                source=source,
                error_code="token_decrypt_failed",
                message=f"Failed to decrypt Firestore token: {exc}",
            )
        ) from exc

    try:
        data = json.loads(plaintext)
    except json.JSONDecodeError as exc:
        raise GoogleAuthFailure(
            CollectorError(
                source=source,
                error_code="token_decrypt_failed",
                message=f"Decrypted token is not valid JSON: {exc}",
            )
        ) from exc

    payload = TokenPayload(
        access_token=data.get("access_token", ""),
        refresh_token=data.get("refresh_token", ""),
        expires_at=int(data.get("expires_at", 0)),
        client_id=data.get("client_id", ""),
        client_secret=data.get("client_secret", ""),
    )

    return payload, doc


def _is_expired(payload: TokenPayload) -> bool:
    """Check if access token is expired (with 5-min buffer)."""
    return payload.expires_at <= int(time.time()) + 300


def _refresh_and_writeback(
    payload: TokenPayload,
    engagement_id: str,
    original_doc: dict[str, object],
) -> TokenPayload:
    """Refresh the access token and write back to Firestore.

    Implements optimistic concurrency: re-reads the doc after refresh,
    skips writeback if updatedAt changed (another writer got there first).
    """
    import requests

    resp = requests.post(  # type: ignore[attr-defined,no-untyped-call]
        "https://oauth2.googleapis.com/token",
        data={
            "client_id": payload.client_id,
            "client_secret": payload.client_secret,
            "refresh_token": payload.refresh_token,
            "grant_type": "refresh_token",
        },
        timeout=15,
    )
    resp.raise_for_status()
    data = resp.json()

    new_access_token = data["access_token"]
    new_expires_in = data.get("expires_in", 3600)
    # Google may rotate refresh_token — use new one if provided
    new_refresh_token = data.get("refresh_token", "") or payload.refresh_token

    refreshed = TokenPayload(
        access_token=new_access_token,
        refresh_token=new_refresh_token,
        expires_at=int(time.time()) + new_expires_in,
        client_id=payload.client_id,
        client_secret=payload.client_secret,
    )

    # Write back to Firestore with optimistic concurrency
    key_info = _get_encryption_key()
    if key_info is not None:
        key_bytes, key_version = key_info
        plaintext = json.dumps({
            "access_token": refreshed.access_token,
            "refresh_token": refreshed.refresh_token,
            "expires_at": refreshed.expires_at,
            "client_id": refreshed.client_id,
            "client_secret": refreshed.client_secret,
        })
        ct_b64, iv_b64 = _encrypt(plaintext, key_bytes)

        from google.cloud.firestore import SERVER_TIMESTAMP

        db = _get_db()
        ref = db.document(f"engagements/{engagement_id}/google_tokens/google")

        # Optimistic concurrency: re-read, skip if doc changed
        current = ref.get()
        if current.exists:
            current_updated = (current.to_dict() or {}).get("updatedAt")
            original_updated = original_doc.get("updatedAt")
            if current_updated != original_updated:
                logger.info(
                    "Firestore token updated by another writer; skipping writeback"
                )
                return refreshed

        ref.set({
            "ciphertext": ct_b64,
            "iv": iv_b64,
            "keyVersion": key_version,
            "updatedAt": SERVER_TIMESTAMP,
            "writtenBy": "heartbeat",
        })
        logger.info("Wrote refreshed token back to Firestore for engagement %s", engagement_id)

    return refreshed


def _payload_to_credentials(payload: TokenPayload) -> Credentials:
    """Convert a TokenPayload to google.oauth2.credentials.Credentials."""
    from google.oauth2.credentials import Credentials

    # Do NOT pass scopes= here. Tauri grants broader scopes (gmail.modify,
    # calendar.events) than the heartbeat's GOOGLE_SCOPES (readonly). If we
    # declare readonly scopes, google-auth may send them during auto-refresh,
    # which can trigger a "scope changed" error from Google. Omitting scopes
    # lets the Credentials object use whatever scopes were originally granted.
    return Credentials(  # type: ignore[no-untyped-call]
        token=payload.access_token,
        refresh_token=payload.refresh_token,
        token_uri="https://oauth2.googleapis.com/token",
        client_id=payload.client_id,
        client_secret=payload.client_secret,
    )


def load_credentials(
    engagement_id: str,
    token_path: Path,
    *,
    source: GoogleSource = "calendar",
) -> Credentials:
    """Load Google credentials, trying Firestore first, then legacy file.

    This is the Phase F replacement for ``load_google_credentials``.
    Callers should migrate from:
        ``load_google_credentials(token_path, source=source)``
    to:
        ``load_credentials(engagement_id, token_path, source=source)``
    """

    # Try Firestore path first
    try:
        result = _read_firestore_token(engagement_id, source)
    except GoogleAuthFailure:
        raise
    except Exception as exc:
        logger.warning("Firestore token read failed (falling back to file): %s", exc)
        result = None

    if result is not None:
        payload, doc = result

        if _is_expired(payload):
            if not payload.refresh_token:
                raise GoogleAuthFailure(
                    CollectorError(
                        source=source,
                        error_code="oauth_refresh_failed",
                        message=(
                            "Firestore token has no refresh_token. "
                            "Re-authenticate in the Tauri app."
                        ),
                    )
                )
            try:
                payload = _refresh_and_writeback(payload, engagement_id, doc)
            except Exception as exc:
                raise GoogleAuthFailure(
                    CollectorError(
                        source=source,
                        error_code="oauth_refresh_failed",
                        message=f"Token refresh failed: {exc}. Re-authenticate in the Tauri app.",
                    )
                ) from exc

        return _payload_to_credentials(payload)

    # Fallback to legacy file-based path
    logger.info("No Firestore token for engagement %s; falling back to token file", engagement_id)
    return load_google_credentials(token_path, source=source)
