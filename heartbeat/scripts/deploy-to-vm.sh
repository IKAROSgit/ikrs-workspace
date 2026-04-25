#!/usr/bin/env bash
#
# Deploy the heartbeat to a remote VM (e.g. Moe's elara-vm).
#
# Strategy: rsync the heartbeat/ tree, then run install.sh remotely.
# Idempotent — re-running upgrades the package without resetting
# operator-captured secrets.
#
# Usage:
#   ./deploy-to-vm.sh user@elara.ikaros.ae
#   ./deploy-to-vm.sh moe@elara-vm.local --rsync-extra='--delete-excluded'
#
# Pre-reqs on the VM:
#   - sudo access for the deploy user.
#   - python3.11+ on PATH.
#   - openssh-server.
#
# Pre-reqs on the operator's Mac (this script runs there):
#   - Already produced token.json via:
#       python3 -m heartbeat.oauth_bootstrap path/to/client_secret.json
#   - Have firebase-sa.json downloaded from the Firebase console.
#   - rsync, ssh, scp installed (Mac default).

set -euo pipefail

if [[ $# -lt 1 ]]; then
  cat >&2 <<EOF
usage: $0 user@host [--rsync-extra='...'] [--source=/path/to/heartbeat]
EOF
  exit 2
fi

REMOTE="$1"
shift

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RSYNC_EXTRA=""

for arg in "$@"; do
  case "$arg" in
    --rsync-extra=*) RSYNC_EXTRA="${arg#--rsync-extra=}" ;;
    --source=*) SOURCE_DIR="${arg#--source=}" ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

say() { printf "\033[1;32m[deploy]\033[0m %s\n" "$*"; }

REMOTE_BASE="/tmp/ikrs-heartbeat-deploy"

say "rsyncing $SOURCE_DIR to $REMOTE:$REMOTE_BASE"
# shellcheck disable=SC2086 # RSYNC_EXTRA is intentionally word-split.
rsync -av --delete \
  --exclude=".venv/" \
  --exclude="__pycache__/" \
  --exclude=".pytest_cache/" \
  --exclude=".mypy_cache/" \
  --exclude=".ruff_cache/" \
  --exclude="*.egg-info/" \
  $RSYNC_EXTRA \
  "$SOURCE_DIR/" \
  "$REMOTE:$REMOTE_BASE/"

say "running install.sh on $REMOTE"
ssh -t "$REMOTE" "sudo HEARTBEAT_SOURCE=$REMOTE_BASE bash $REMOTE_BASE/scripts/install.sh"

say "deploy complete. Tail logs with:"
say "  ssh $REMOTE 'sudo journalctl -u ikrs-heartbeat -f'"
