#!/usr/bin/env bash
# local-ad-hoc-sign.sh — Build + ad-hoc sign IKAROS Workspace for daily use
#
# Use this while waiting on Apple Developer enrolment. The produced .app
# runs on your own Mac (right-click → Open on first launch) but cannot
# be distributed to other users. For external distribution, you need a
# real Developer ID cert and notarization — see SECURITY.md.
#
# Ad-hoc signatures use the identity `-`, which macOS treats as
# "self-signed, local-only." Gatekeeper blocks first launch unless you
# right-click → Open (one-time override). Subsequent launches work
# normally.
#
# Usage:
#   ./tools/scripts/local-ad-hoc-sign.sh          # build + sign
#   ./tools/scripts/local-ad-hoc-sign.sh install  # build + sign + copy to /Applications
#
set -euo pipefail

# Resolve repo root regardless of where the script is invoked from
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this script targets macOS (uname: $(uname -s))" >&2
  exit 2
fi

# Pre-flight: ensure Tauri CLI is callable (via npx)
if ! npx --no-install tauri --version >/dev/null 2>&1; then
  echo "error: Tauri CLI not available via npx. Run 'npm ci' first." >&2
  exit 2
fi

# Build the release bundle. Unsigned at this point — tauri.conf.json
# sets signingIdentity to "-" which Tauri's bundler treats as "skip my
# signing step." We do the ad-hoc sign ourselves below so we also
# strip quarantine.
echo "==> Building release bundle..."
npx tauri build

APP_DIR="src-tauri/target/release/bundle/macos"
APP_PATH="$APP_DIR/IKAROS Workspace.app"

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: expected bundle at $APP_PATH not found. Did 'tauri build' fail?" >&2
  echo "Contents of $APP_DIR:" >&2
  ls -la "$APP_DIR" 2>/dev/null || echo "(directory does not exist)" >&2
  exit 1
fi

echo "==> Ad-hoc signing $APP_PATH"
codesign --force --deep --sign - "$APP_PATH"

# Strip quarantine so Gatekeeper doesn't prompt on first launch from
# this build. (New copies downloaded later will still get quarantined —
# this is only a convenience for the copy we just built.)
xattr -d com.apple.quarantine "$APP_PATH" 2>/dev/null || true

echo "==> Verifying ad-hoc signature"
codesign --verify --verbose=2 "$APP_PATH" || {
  echo "error: signature verification failed" >&2
  exit 1
}

SIZE=$(du -sh "$APP_PATH" | cut -f1)
echo ""
echo "Built and ad-hoc signed: $APP_PATH ($SIZE)"
echo ""

if [[ "${1:-}" == "install" ]]; then
  DEST="/Applications/IKAROS Workspace.app"
  if [[ -d "$DEST" ]]; then
    echo "==> Removing existing $DEST"
    rm -rf "$DEST"
  fi
  echo "==> Copying to /Applications"
  cp -R "$APP_PATH" "$DEST"
  echo ""
  echo "Installed. First launch:"
  echo "  right-click (or ctrl-click) the app in /Applications → Open → 'Open' in the dialog"
  echo "Subsequent launches: double-click as normal."
else
  echo "To install: re-run with 'install' argument:"
  echo "  ./tools/scripts/local-ad-hoc-sign.sh install"
fi
