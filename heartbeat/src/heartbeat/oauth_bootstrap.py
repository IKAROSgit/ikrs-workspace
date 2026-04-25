"""Mac-side OAuth bootstrap.

Runs once on the operator's Mac during install. Spawns a local browser,
captures the OAuth consent for Calendar + Gmail (read-only), and writes
``token.json`` to the working directory.

The operator then scp's the resulting ``token.json`` to the VM at
``/etc/ikrs-heartbeat/google-token.json``.

Usage (on the Mac):

    cd ikrs-workspace/heartbeat
    python3.11 -m venv .venv
    .venv/bin/pip install -e .
    .venv/bin/python -m heartbeat.oauth_bootstrap path/to/client_secret.json
    scp token.json vm:/etc/ikrs-heartbeat/google-token.json

The ``client_secret.json`` comes from Google Cloud Console → APIs &
Services → Credentials → "Create credentials" → "OAuth client ID" →
type "Desktop app". Download the JSON, pass the path here.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from heartbeat.signals.google_auth import GOOGLE_SCOPES


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="ikrs-heartbeat-oauth-bootstrap",
        description=(
            "Run the Google OAuth installed-app flow on the operator's "
            "Mac to produce token.json for the VM."
        ),
    )
    parser.add_argument(
        "client_secret",
        type=Path,
        help="path to client_secret.json downloaded from Google Cloud Console",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("token.json"),
        help="where to write the token (default: ./token.json)",
    )
    args = parser.parse_args(argv)

    if not args.client_secret.exists():
        print(
            f"client_secret file not found: {args.client_secret}",
            file=sys.stderr,
        )
        return 2

    try:
        from google_auth_oauthlib.flow import InstalledAppFlow
    except ImportError:
        print(
            "google-auth-oauthlib not installed. Run: pip install -e .",
            file=sys.stderr,
        )
        return 2

    flow = InstalledAppFlow.from_client_secrets_file(  # type: ignore[no-untyped-call]
        str(args.client_secret), GOOGLE_SCOPES
    )
    # ``access_type=offline`` and ``prompt=consent`` together guarantee a
    # refresh_token in the response, even if the user has previously
    # consented to these scopes (Google omits refresh_token on re-consent
    # without prompt=consent).
    creds = flow.run_local_server(
        port=0,
        access_type="offline",
        prompt="consent",
    )
    args.out.write_text(creds.to_json())
    args.out.chmod(0o600)

    print(f"wrote {args.out} ({args.out.stat().st_size} bytes)")
    print("scp this to the VM:")
    print(f"  scp {args.out} vm:/etc/ikrs-heartbeat/google-token.json")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
