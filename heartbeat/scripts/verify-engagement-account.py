#!/usr/bin/env python3
"""Verify which Google account is connected to a given engagement.

Decrypts the Firestore token, refreshes if needed, calls Gmail
users.getProfile(userId="me"), prints the emailAddress.

Usage:
  source /etc/ikrs-heartbeat/secrets.env
  export TOKEN_ENCRYPTION_KEY TOKEN_ENCRYPTION_KEY_VERSION FIREBASE_SA_KEY_PATH
  sudo -E /opt/ikrs-heartbeat/venv/bin/python verify-engagement-account.py <engagement_id>
"""

from __future__ import annotations

import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 2:
        print(f"usage: {sys.argv[0]} <engagement_id>", file=sys.stderr)
        return 2

    engagement_id = sys.argv[1]

    from heartbeat.signals.firestore_tokens import load_credentials
    from googleapiclient.discovery import build

    creds = load_credentials(
        engagement_id,
        Path("/etc/ikrs-heartbeat/google-token.json"),
        source="verify",
    )
    svc = build("gmail", "v1", credentials=creds, cache_discovery=False)
    profile = svc.users().getProfile(userId="me").execute()
    email = profile.get("emailAddress", "unknown")
    print(f"emailAddress: {email}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
