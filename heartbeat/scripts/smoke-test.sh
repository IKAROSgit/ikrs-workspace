#!/usr/bin/env bash
#
# Quick post-install verification.
#
# Runs the heartbeat in --dry-run mode (no network calls), checks the
# systemd timer is active, and confirms the most recent tick (if any)
# completed successfully.
#
# Exit codes:
#   0  — all checks passed.
#   1  — dry-run failed.
#   2  — timer not active.
#   3  — last tick errored (warning only — exit 0 unless --strict).
#
# Usage:
#   ./smoke-test.sh             # operator-friendly: warns on tick errors
#   ./smoke-test.sh --strict    # fail hard on tick errors

set -euo pipefail

VENV_DIR="/opt/ikrs-heartbeat/venv"
ETC_DIR="/etc/ikrs-heartbeat"
IKRS_USER="${IKRS_USER:-ikrs}"
STRICT=false

for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=true ;;
  esac
done

say() { printf "\033[1;32m[smoke]\033[0m %s\n" "$*"; }
warn() { printf "\033[1;33m[smoke]\033[0m %s\n" "$*" >&2; }
fail() { printf "\033[1;31m[smoke]\033[0m %s\n" "$*" >&2; exit "$1"; }

# --------------------------------------------------------------------------- #
# 1. Dry run
# --------------------------------------------------------------------------- #

say "running ikrs-heartbeat --dry-run"
if [[ "$EUID" -eq 0 ]]; then
  sudo -u "$IKRS_USER" "$VENV_DIR/bin/ikrs-heartbeat" \
    --dry-run \
    --config "$ETC_DIR/heartbeat.toml" \
    --token-path "$ETC_DIR/google-token.json" \
    || fail 1 "dry-run failed"
else
  "$VENV_DIR/bin/ikrs-heartbeat" \
    --dry-run \
    --config "$ETC_DIR/heartbeat.toml" \
    --token-path "$ETC_DIR/google-token.json" \
    || fail 1 "dry-run failed (try: sudo $0)"
fi
say "dry-run OK"

# --------------------------------------------------------------------------- #
# 2. Timer state
# --------------------------------------------------------------------------- #

if systemctl is-active --quiet ikrs-heartbeat.timer; then
  next="$(systemctl show ikrs-heartbeat.timer -p NextElapseRealtimestamp --value)"
  say "timer active. Next fire: $next"
else
  fail 2 "timer is NOT active. Try: sudo systemctl enable --now ikrs-heartbeat.timer"
fi

# --------------------------------------------------------------------------- #
# 3. Last tick outcome (best effort)
# --------------------------------------------------------------------------- #

last_state="$(systemctl show ikrs-heartbeat.service -p ExecMainStatus --value 2>/dev/null || echo "")"
last_active="$(systemctl show ikrs-heartbeat.service -p ActiveEnterTimestamp --value 2>/dev/null || echo "")"

if [[ -z "$last_active" || "$last_active" == "n/a" ]]; then
  say "no ticks have fired yet (fresh install) — wait for the next scheduled run"
elif [[ "$last_state" != "0" ]]; then
  msg="last tick exited with status $last_state at $last_active"
  if $STRICT; then
    fail 3 "$msg"
  else
    warn "$msg"
    warn "investigate: sudo journalctl -u ikrs-heartbeat -n 100"
  fi
else
  say "last tick succeeded at $last_active"
fi

say "smoke-test passed."
