#!/usr/bin/env python3
"""Phase F migration: upload legacy google-token.json to Firestore.

Reads the Phase E token file, translates to the Tauri TokenPayload
schema, encrypts with AES-256-GCM, writes to
engagements/{eid}/google_tokens/google in Firestore, and verifies
via immediate read-back + decrypt.

Idempotent: skips if a Firestore doc already exists and its plaintext
matches the local file.

Exit codes:
  0 = success (or already migrated)
  1 = legacy token file missing
  2 = encryption key missing / malformed
  3 = Firestore write failed
  4 = read-back verification failed
"""

from __future__ import annotations

import argparse
import base64
import json
import logging
import os
import sys
from datetime import UTC, datetime
from pathlib import Path

logger = logging.getLogger("migrate-token")

_DEFAULT_TOKEN_PATH = Path("/etc/ikrs-heartbeat/google-token.json")


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="migrate-token-to-firestore",
        description="Migrate legacy google-token.json to Firestore (Phase F).",
    )
    parser.add_argument(
        "engagement_id",
        help="Firestore engagement document ID (e.g. 5L12siRpQDDXnPCk892H).",
    )
    parser.add_argument(
        "--token-path",
        type=Path,
        default=_DEFAULT_TOKEN_PATH,
        help=f"Path to legacy google-token.json (default: {_DEFAULT_TOKEN_PATH}).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Do everything except the Firestore write.",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Verbose logging (DEBUG level).",
    )
    return parser.parse_args(argv)


def _get_encryption_key() -> tuple[bytes, int] | None:
    """Read TOKEN_ENCRYPTION_KEY + VERSION from env."""
    key_b64 = os.environ.get("TOKEN_ENCRYPTION_KEY", "")
    if not key_b64:
        return None
    try:
        key_bytes = base64.b64decode(key_b64)
    except Exception:
        logger.error("TOKEN_ENCRYPTION_KEY is not valid base64")
        return None
    if len(key_bytes) != 32:
        logger.error("TOKEN_ENCRYPTION_KEY decoded to %d bytes, expected 32", len(key_bytes))
        return None
    version = int(os.environ.get("TOKEN_ENCRYPTION_KEY_VERSION", "1"))
    return key_bytes, version


def _translate_legacy_token(legacy: dict[str, object]) -> dict[str, object]:
    """Translate google-auth SDK format to Tauri TokenPayload schema.

    Mapping:
      token           -> access_token
      refresh_token   -> refresh_token (same)
      expiry (ISO)    -> expires_at (Unix epoch int)
      client_id       -> client_id (same)
      client_secret   -> client_secret (same)
    Dropped: token_uri, scopes, universe_domain, account
    """
    expiry_str = str(legacy.get("expiry", ""))
    if expiry_str:
        try:
            dt = datetime.fromisoformat(expiry_str)
            if dt.tzinfo is None:
                dt = dt.replace(tzinfo=UTC)
            expires_at = int(dt.timestamp())
        except ValueError:
            expires_at = 0
    else:
        expires_at = 0

    return {
        "access_token": str(legacy.get("token", "")),
        "refresh_token": str(legacy.get("refresh_token", "")),
        "expires_at": expires_at,
        "client_id": str(legacy.get("client_id", "")),
        "client_secret": str(legacy.get("client_secret", "")),
    }


def _encrypt(plaintext: str, key_bytes: bytes) -> tuple[str, str]:
    """AES-256-GCM encrypt. Returns (ciphertext_b64, iv_b64)."""
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM

    iv = os.urandom(12)
    aesgcm = AESGCM(key_bytes)
    ct = aesgcm.encrypt(iv, plaintext.encode("utf-8"), None)
    return base64.b64encode(ct).decode("ascii"), base64.b64encode(iv).decode("ascii")


def _decrypt(ciphertext_b64: str, iv_b64: str, key_bytes: bytes) -> str:
    """AES-256-GCM decrypt. Returns plaintext string."""
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM

    ct = base64.b64decode(ciphertext_b64)
    iv = base64.b64decode(iv_b64)
    aesgcm = AESGCM(key_bytes)
    return aesgcm.decrypt(iv, ct, None).decode("utf-8")


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )

    # 1. Check legacy token file
    if not args.token_path.exists():
        logger.error("Legacy token file not found: %s", args.token_path)
        return 1

    # 2. Check encryption key
    key_info = _get_encryption_key()
    if key_info is None:
        logger.error(
            "TOKEN_ENCRYPTION_KEY not set or malformed in environment. "
            "Source /etc/ikrs-heartbeat/secrets.env or export it."
        )
        return 2
    key_bytes, key_version = key_info

    # 3. Read + translate legacy token
    try:
        with args.token_path.open() as f:
            legacy = json.load(f)
    except (json.JSONDecodeError, OSError) as exc:
        logger.error("Failed to read legacy token: %s", exc)
        return 1

    payload = _translate_legacy_token(legacy)
    plaintext = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    logger.info("Translated legacy token for engagement %s", args.engagement_id)
    logger.info(
        "  access_token: %s...  refresh_token: %s...  expires_at: %s",
        payload["access_token"][:10] if payload["access_token"] else "(empty)",
        str(payload["refresh_token"])[:10] if payload["refresh_token"] else "(empty)",
        payload["expires_at"],
    )

    # 4. Init Firebase Admin SDK
    import firebase_admin
    from firebase_admin import firestore as fs

    if not firebase_admin._apps:
        sa_path = os.environ.get(
            "FIREBASE_SA_KEY_PATH", "/etc/ikrs-heartbeat/firebase-sa.json"
        )
        from firebase_admin import credentials

        cred = credentials.Certificate(sa_path)
        firebase_admin.initialize_app(cred)

    db = fs.client()
    ref = db.document(
        f"engagements/{args.engagement_id}/google_tokens/google"
    )

    # 5. Idempotency: check if already migrated
    existing = ref.get()
    if existing.exists:
        doc = existing.to_dict() or {}
        ct_b64 = doc.get("ciphertext", "")
        iv_b64 = doc.get("iv", "")
        doc_key_version = doc.get("keyVersion")
        if ct_b64 and iv_b64 and doc_key_version == key_version:
            try:
                existing_plaintext = _decrypt(ct_b64, iv_b64, key_bytes)
                existing_payload = json.loads(existing_plaintext)
                # Compare refresh_token — that's the critical field
                if existing_payload.get("refresh_token") == payload["refresh_token"]:
                    logger.info(
                        "Already migrated: Firestore doc exists with matching "
                        "refresh_token. Nothing to do."
                    )
                    return 0
                logger.info(
                    "Firestore doc exists but refresh_token differs. "
                    "Re-migrating with current legacy token."
                )
            except Exception as exc:
                logger.info(
                    "Firestore doc exists but decrypt failed (%s). "
                    "Re-migrating.", exc
                )

    # 6. Encrypt
    ct_b64, iv_b64 = _encrypt(plaintext, key_bytes)
    logger.info("Encrypted token (keyVersion=%d)", key_version)

    # 7. Write (or dry-run)
    if args.dry_run:
        logger.info(
            "DRY RUN: would write to engagements/%s/google_tokens/google "
            "(ciphertext=%d chars, iv=%d chars, keyVersion=%d)",
            args.engagement_id,
            len(ct_b64),
            len(iv_b64),
            key_version,
        )
        logger.info("DRY RUN: no Firestore write performed.")
        return 0

    try:
        from google.cloud.firestore import SERVER_TIMESTAMP

        ref.set({
            "ciphertext": ct_b64,
            "iv": iv_b64,
            "keyVersion": key_version,
            "updatedAt": SERVER_TIMESTAMP,
            "writtenBy": "migration",
        })
        logger.info("Wrote encrypted token to Firestore.")
    except Exception as exc:
        logger.error("Firestore write failed: %s", exc)
        return 3

    # 8. Read-back verification
    try:
        verify_snap = ref.get()
        if not verify_snap.exists:
            logger.error("Verification failed: doc not found after write.")
            return 4
        verify_doc = verify_snap.to_dict() or {}
        verify_plaintext = _decrypt(
            verify_doc["ciphertext"], verify_doc["iv"], key_bytes
        )
        verify_payload = json.loads(verify_plaintext)

        # Compare all fields
        for field in ("access_token", "refresh_token", "expires_at", "client_id", "client_secret"):
            local_val = payload[field]
            remote_val = verify_payload.get(field)
            if str(local_val) != str(remote_val):
                logger.error(
                    "Verification failed: field %s mismatch. "
                    "Local=%s Remote=%s. "
                    "Legacy token.json is untouched; fix and retry.",
                    field,
                    str(local_val)[:20],
                    str(remote_val)[:20] if remote_val else "(missing)",
                )
                return 4

        logger.info("Read-back verification passed. All fields match.")
    except Exception as exc:
        logger.error(
            "Verification failed: %s. "
            "Legacy token.json is untouched; fix and retry.",
            exc,
        )
        return 4

    # 9. Success
    logger.info("")
    logger.info("Migration complete for engagement %s.", args.engagement_id)
    logger.info(
        "Legacy token at %s is preserved. After the next heartbeat tick "
        "passes successfully, you may remove it:",
        args.token_path,
    )
    logger.info("  sudo rm %s", args.token_path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
