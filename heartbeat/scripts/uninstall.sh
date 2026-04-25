#!/usr/bin/env bash
#
# Stop + disable the heartbeat timer/service. Removes systemd units and
# the venv. Preserves /etc/ikrs-heartbeat (config + secrets) and
# /var/lib/ikrs-heartbeat (state) by default — pass --purge to wipe
# everything.
#
# Usage:
#   sudo ./uninstall.sh
#   sudo ./uninstall.sh --purge

set -euo pipefail

ETC_DIR="/etc/ikrs-heartbeat"
STATE_DIR="/var/lib/ikrs-heartbeat"
VENV_DIR="/opt/ikrs-heartbeat/venv"
SYSTEMD_DIR="/etc/systemd/system"
IKRS_USER="${IKRS_USER:-ikrs}"

PURGE=false
for arg in "$@"; do
  case "$arg" in
    --purge) PURGE=true ;;
    -h|--help)
      sed -n '2,/^$/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

say() { printf "\033[1;32m[uninstall]\033[0m %s\n" "$*"; }

[[ "$EUID" -eq 0 ]] || { echo "must run as root" >&2; exit 1; }

say "stopping + disabling timer + service"
systemctl disable --now ikrs-heartbeat.timer 2>/dev/null || true
systemctl disable --now ikrs-heartbeat.service 2>/dev/null || true

say "removing systemd units"
rm -f "$SYSTEMD_DIR/ikrs-heartbeat.timer" "$SYSTEMD_DIR/ikrs-heartbeat.service"
systemctl daemon-reload

say "removing virtualenv"
rm -rf "$(dirname "$VENV_DIR")"

if $PURGE; then
  say "PURGE: wiping config, secrets, and state"
  rm -rf "$ETC_DIR" "$STATE_DIR"
  if id -u "$IKRS_USER" >/dev/null 2>&1; then
    say "removing user $IKRS_USER"
    userdel --remove "$IKRS_USER" 2>/dev/null || true
  fi
else
  say "preserving $ETC_DIR (config + secrets) and $STATE_DIR (state)"
  say "  pass --purge to wipe them too"
fi

say "uninstall complete."
