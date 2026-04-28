#!/usr/bin/env bash
#
# IKAROS Heartbeat Tier II installer.
#
# Idempotent: re-running upgrades the package + refreshes systemd units
# without clobbering operator secrets. Safe to run inside ansible /
# Terraform provisioners.
#
# What this does:
#   1. Creates ikrs user + /etc/ikrs-heartbeat/, /var/lib/ikrs-heartbeat/.
#   2. Creates a Python 3.11+ virtualenv at /opt/ikrs-heartbeat/venv.
#   3. Installs the heartbeat package (this checked-out tree by default;
#      override with HEARTBEAT_SOURCE=/path/to/wheel).
#   4. Captures missing secrets interactively (Gemini, Firebase SA path,
#      Telegram bot token + chat_id) — preserves existing values.
#   5. Runs Telegram deleteWebhook so getUpdates works for the operator's
#      bot (handles the case where the token was previously webhooked).
#   6. Installs systemd unit + timer, enables them.
#   7. Runs a dry-run smoke test before exiting.
#
# Usage:
#   sudo ./install.sh
#
# Env overrides (any of these, all optional):
#   HEARTBEAT_SOURCE     — path to heartbeat/ checkout or wheel.
#                          Default: directory of this script's parent.
#   GEMINI_API_KEY       — pre-seed instead of prompting.
#   TELEGRAM_BOT_TOKEN   — same.
#   TELEGRAM_CHAT_ID     — same.
#   FIREBASE_SA_KEY_PATH — must already exist on disk (we copy it into
#                          /etc/ikrs-heartbeat/firebase-sa.json).
#   IKRS_USER            — service user name. Default: ikrs.
#   VAULT_ROOT           — operator vault path. Default: prompted.

set -euo pipefail

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

IKRS_USER="${IKRS_USER:-ikrs}"
ETC_DIR="/etc/ikrs-heartbeat"
STATE_DIR="/var/lib/ikrs-heartbeat"
VENV_DIR="/opt/ikrs-heartbeat/venv"
SYSTEMD_DIR="/etc/systemd/system"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HEARTBEAT_SOURCE="${HEARTBEAT_SOURCE:-$(cd "$SCRIPT_DIR/.." && pwd)}"

# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #

say() { printf "\033[1;32m[install]\033[0m %s\n" "$*"; }
warn() { printf "\033[1;33m[install]\033[0m %s\n" "$*" >&2; }
die() { printf "\033[1;31m[install]\033[0m %s\n" "$*" >&2; exit 1; }

require_root() {
  [[ "$EUID" -eq 0 ]] || die "must run as root (try: sudo $0)"
}

require_command() {
  for c in "$@"; do
    command -v "$c" >/dev/null 2>&1 || die "missing required command: $c"
  done
}

# Prompt for a secret only if not already set + not present in secrets.env.
# $1: env-var name. $2: prompt label. $3: existing-secrets file path.
#
# Security: never use `eval` on file content. A secrets.env value
# containing `$(...)` or backticks would otherwise execute as shell when
# we re-read it. Strip surrounding quotes manually with parameter
# expansion, then assign via `printf -v` (which does NOT evaluate).
read_secret_if_missing() {
  local var="$1" label="$2" existing="$3"
  if [[ -n "${!var:-}" ]]; then
    return 0
  fi
  if [[ -f "$existing" ]] && grep -q "^${var}=" "$existing"; then
    # Pull the raw line, take everything after the first `=`, strip
    # at-most-one pair of surrounding double quotes. No shell expansion.
    local raw_value
    raw_value="$(grep "^${var}=" "$existing" | head -n 1 | cut -d= -f2-)"
    # Strip optional leading/trailing double quote.
    raw_value="${raw_value#\"}"
    raw_value="${raw_value%\"}"
    printf -v "$var" "%s" "$raw_value"
    return 0
  fi
  printf "[install] %s: " "$label"
  local value
  read -r value
  printf -v "$var" "%s" "$value"
}

# --------------------------------------------------------------------------- #
# 0. Pre-flight
# --------------------------------------------------------------------------- #

require_root
require_command python3 systemctl curl

PY="$(command -v python3.11 || command -v python3.12 || command -v python3.13 || command -v python3 || true)"
if [[ -z "$PY" ]]; then
  die "no python3.11+ found on PATH. Install python3.11 or set PY=/path/to/python3"
fi
PY_VERSION="$($PY -c 'import sys; print(f"{sys.version_info[0]}.{sys.version_info[1]}")')"
if ! printf '%s\n3.11\n' "$PY_VERSION" | sort -V -c >/dev/null 2>&1; then
  die "python >=3.11 required; found $PY_VERSION"
fi

say "using python at $PY ($PY_VERSION)"
say "using heartbeat source at $HEARTBEAT_SOURCE"

# --------------------------------------------------------------------------- #
# 1. User + dirs
# --------------------------------------------------------------------------- #

if ! id -u "$IKRS_USER" >/dev/null 2>&1; then
  say "creating system user $IKRS_USER"
  useradd --system --home /var/lib/ikrs-heartbeat --shell /usr/sbin/nologin "$IKRS_USER"
else
  say "user $IKRS_USER already exists"
fi

install -d -m 0750 -o "$IKRS_USER" -g "$IKRS_USER" "$ETC_DIR"
install -d -m 0750 -o "$IKRS_USER" -g "$IKRS_USER" "$STATE_DIR"
install -d -m 0755 -o root -g root "$(dirname "$VENV_DIR")"

# --------------------------------------------------------------------------- #
# 2. Virtualenv + heartbeat package
# --------------------------------------------------------------------------- #

if [[ ! -x "$VENV_DIR/bin/python" ]]; then
  say "creating virtualenv at $VENV_DIR"
  "$PY" -m venv "$VENV_DIR"
fi

say "upgrading pip"
"$VENV_DIR/bin/pip" install --upgrade pip --quiet

say "installing heartbeat package from $HEARTBEAT_SOURCE"
"$VENV_DIR/bin/pip" install "$HEARTBEAT_SOURCE" --quiet

# venv must be readable by the service user.
chown -R "$IKRS_USER:$IKRS_USER" "$VENV_DIR"

# --------------------------------------------------------------------------- #
# 3. heartbeat.toml
# --------------------------------------------------------------------------- #

if [[ -f "$ETC_DIR/heartbeat.toml" ]]; then
  say "preserving existing $ETC_DIR/heartbeat.toml"
else
  say "writing default $ETC_DIR/heartbeat.toml"
  if [[ -z "${VAULT_ROOT:-}" ]]; then
    printf "[install] absolute path to engagement vault on this VM: "
    read -r VAULT_ROOT
  fi
  if [[ -z "${TENANT_ID:-}" ]]; then
    printf "[install] tenant ID (e.g. moe-ikaros-ae): "
    read -r TENANT_ID
  fi
  if [[ -z "${ENGAGEMENT_ID:-}" ]]; then
    printf "[install] engagement ID (Firestore doc ID, e.g. 5L12siRpQDDXnPCk892H): "
    read -r ENGAGEMENT_ID
  fi
  if [[ -z "${FIRESTORE_PROJECT_ID:-}" ]]; then
    printf "[install] Firebase project ID: "
    read -r FIRESTORE_PROJECT_ID
  fi
  install -m 0640 -o "$IKRS_USER" -g "$IKRS_USER" /dev/null "$ETC_DIR/heartbeat.toml"
  cat > "$ETC_DIR/heartbeat.toml" <<EOF
tenant_id = "$TENANT_ID"
prompt_version = "tick_prompt.v1"

[[engagements]]
id = "$ENGAGEMENT_ID"
vault_root = "$VAULT_ROOT"

[llm]
provider = "gemini"
model = "gemini-2.5-pro"
temperature = 0.2
max_output_tokens = 4096

[signals]
calendar_enabled = true
gmail_enabled = true
vault_enabled = true
calendar_lookahead_hours = 24
gmail_lookback_hours = 24

[outputs]
firestore_enabled = true
telegram_enabled = true
audit_enabled = true
firestore_project_id = "$FIRESTORE_PROJECT_ID"
EOF
  chown "$IKRS_USER:$IKRS_USER" "$ETC_DIR/heartbeat.toml"
  chmod 0640 "$ETC_DIR/heartbeat.toml"
fi

# --------------------------------------------------------------------------- #
# 4. Secrets
# --------------------------------------------------------------------------- #

SECRETS_FILE="$ETC_DIR/secrets.env"
if [[ ! -f "$SECRETS_FILE" ]]; then
  install -m 0600 -o "$IKRS_USER" -g "$IKRS_USER" /dev/null "$SECRETS_FILE"
fi

read_secret_if_missing GEMINI_API_KEY "Gemini API key (AI Studio)" "$SECRETS_FILE"
read_secret_if_missing TELEGRAM_BOT_TOKEN "Telegram bot token (BotFather)" "$SECRETS_FILE"
read_secret_if_missing TELEGRAM_CHAT_ID "Telegram chat ID (after messaging the bot once)" "$SECRETS_FILE"

# Firebase SA — copy the file rather than store its contents in env.
if [[ -n "${FIREBASE_SA_KEY_PATH:-}" ]]; then
  if [[ ! -f "$FIREBASE_SA_KEY_PATH" ]]; then
    die "FIREBASE_SA_KEY_PATH=$FIREBASE_SA_KEY_PATH does not exist"
  fi
  install -m 0600 -o "$IKRS_USER" -g "$IKRS_USER" "$FIREBASE_SA_KEY_PATH" "$ETC_DIR/firebase-sa.json"
elif [[ ! -f "$ETC_DIR/firebase-sa.json" ]]; then
  warn "no Firebase SA at $ETC_DIR/firebase-sa.json — copy it manually:"
  warn "  sudo install -m 0600 -o $IKRS_USER -g $IKRS_USER /path/to/sa.json $ETC_DIR/firebase-sa.json"
fi

# Phase F: Token encryption key for Firestore-synced OAuth tokens.
# Generate if not already set or present in secrets.env.
TOKEN_ENCRYPTION_KEY="${TOKEN_ENCRYPTION_KEY:-}"
TOKEN_ENCRYPTION_KEY_VERSION="${TOKEN_ENCRYPTION_KEY_VERSION:-1}"
if [[ -z "$TOKEN_ENCRYPTION_KEY" ]] && [[ -f "$SECRETS_FILE" ]] && grep -q "^TOKEN_ENCRYPTION_KEY=" "$SECRETS_FILE"; then
  TOKEN_ENCRYPTION_KEY="$(grep "^TOKEN_ENCRYPTION_KEY=" "$SECRETS_FILE" | head -n 1 | cut -d= -f2- | tr -d '"')"
  TOKEN_ENCRYPTION_KEY_VERSION="$(grep "^TOKEN_ENCRYPTION_KEY_VERSION=" "$SECRETS_FILE" | head -n 1 | cut -d= -f2- | tr -d '"' || echo 1)"
fi
if [[ -z "$TOKEN_ENCRYPTION_KEY" ]]; then
  say "generating AES-256-GCM token encryption key (Phase F)"
  TOKEN_ENCRYPTION_KEY="$(openssl rand -base64 32)"
  TOKEN_ENCRYPTION_KEY_VERSION="1"
  # Persist immediately so the key survives a crash before the full
  # secrets.env rewrite below. Avoids TOCTOU: key displayed but not saved.
  if [[ -f "$SECRETS_FILE" ]]; then
    # Append to existing file
    printf 'TOKEN_ENCRYPTION_KEY="%s"\nTOKEN_ENCRYPTION_KEY_VERSION="%s"\n' \
      "$TOKEN_ENCRYPTION_KEY" "$TOKEN_ENCRYPTION_KEY_VERSION" >> "$SECRETS_FILE"
  fi
  echo ""
  echo "================================================================"
  echo "  BACK UP THIS KEY — it encrypts all OAuth tokens in Firestore."
  echo "  If you lose it, ALL encrypted tokens become unreadable and"
  echo "  operators must re-authenticate via the Tauri app."
  echo ""
  echo "  TOKEN_ENCRYPTION_KEY=$TOKEN_ENCRYPTION_KEY"
  echo ""
  echo "  Copy this key to your Mac's .env.local as:"
  echo "  VITE_TOKEN_ENCRYPTION_KEY=$TOKEN_ENCRYPTION_KEY"
  echo "================================================================"
  echo ""
  printf "[install] Press ENTER to acknowledge you have backed up the key: "
  read -r _
fi

# Phase F: detect upgrade-from-Phase-E scenario.
# If legacy google-token.json exists but no encryption key was previously set,
# the operator needs to run the migration script after install completes.
NEEDS_TOKEN_MIGRATION=false
if [[ -f "$ETC_DIR/google-token.json" ]]; then
  say "legacy google-token.json found (Phase E). After install, migrate with:"
  say "  source $SECRETS_FILE && export TOKEN_ENCRYPTION_KEY TOKEN_ENCRYPTION_KEY_VERSION"
  say "  sudo -E $VENV_DIR/bin/python $HEARTBEAT_SOURCE/scripts/migrate-token-to-firestore.py <engagement-id>"
  NEEDS_TOKEN_MIGRATION=true
else
  say "no legacy google-token.json — fresh install (Phase F)."
  say "operator must connect Google in the Tauri app for each engagement."
fi

# Now write secrets.env (idempotent rewrite — preserves prior values).
cat > "$SECRETS_FILE" <<EOF
GEMINI_API_KEY="$GEMINI_API_KEY"
TELEGRAM_BOT_TOKEN="$TELEGRAM_BOT_TOKEN"
TELEGRAM_CHAT_ID="$TELEGRAM_CHAT_ID"
FIREBASE_SA_KEY_PATH="$ETC_DIR/firebase-sa.json"
TOKEN_ENCRYPTION_KEY="$TOKEN_ENCRYPTION_KEY"
TOKEN_ENCRYPTION_KEY_VERSION="$TOKEN_ENCRYPTION_KEY_VERSION"
EOF
chown "$IKRS_USER:$IKRS_USER" "$SECRETS_FILE"
chmod 0600 "$SECRETS_FILE"

# --------------------------------------------------------------------------- #
# 5. Telegram deleteWebhook (handles previously-webhooked tokens)
# --------------------------------------------------------------------------- #

if [[ -n "$TELEGRAM_BOT_TOKEN" ]]; then
  say "deleting any pre-existing Telegram webhook (so getUpdates works)"
  curl -sS -X POST \
    "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/deleteWebhook" \
    -d "drop_pending_updates=true" \
    | sed 's/^/[install] telegram: /' || warn "deleteWebhook failed; continuing"
fi

# --------------------------------------------------------------------------- #
# 6. systemd
# --------------------------------------------------------------------------- #

say "installing systemd units"
install -m 0644 "$HEARTBEAT_SOURCE/systemd/ikrs-heartbeat.service" "$SYSTEMD_DIR/ikrs-heartbeat.service"
install -m 0644 "$HEARTBEAT_SOURCE/systemd/ikrs-heartbeat.timer" "$SYSTEMD_DIR/ikrs-heartbeat.timer"

# Append a vault-root ReadWritePaths= line to the unit if one is set.
VAULT_ROOT_FROM_TOML="$(grep -E '^vault_root\s*=' "$ETC_DIR/heartbeat.toml" | head -1 | cut -d'=' -f2- | tr -d ' "')"
if [[ -n "$VAULT_ROOT_FROM_TOML" ]]; then
  if ! grep -q "ReadWritePaths=$VAULT_ROOT_FROM_TOML" "$SYSTEMD_DIR/ikrs-heartbeat.service"; then
    say "adding vault path to ReadWritePaths"
    sed -i "/^ReadWritePaths=\/var\/lib\/ikrs-heartbeat/a ReadWritePaths=$VAULT_ROOT_FROM_TOML" \
      "$SYSTEMD_DIR/ikrs-heartbeat.service"
  fi

  # If vault is under /home/*, ProtectHome=true would hide it from the
  # service entirely (systemd namespace isolation overrides
  # ReadWritePaths and filesystem perms). Flip ProtectHome=false in that
  # case so the heartbeat can read+write the vault. For vaults outside
  # /home (e.g. /var/lib/...), keep ProtectHome=true for tighter
  # isolation.
  if [[ "$VAULT_ROOT_FROM_TOML" == /home/* ]]; then
    say "vault is under /home/ — relaxing ProtectHome=true → ProtectHome=false"
    sed -i 's|^ProtectHome=true|ProtectHome=false|' \
      "$SYSTEMD_DIR/ikrs-heartbeat.service"
  fi
fi

systemctl daemon-reload
systemctl enable --now ikrs-heartbeat.timer

# --------------------------------------------------------------------------- #
# 7. Smoke test
# --------------------------------------------------------------------------- #

say "running --dry-run smoke test"
sudo -u "$IKRS_USER" "$VENV_DIR/bin/ikrs-heartbeat" \
  --dry-run \
  --config "$ETC_DIR/heartbeat.toml" \
  --token-path "$ETC_DIR/google-token.json" \
  || die "dry-run smoke test failed"

say "install complete."
say "  next tick fires:           $(systemctl show ikrs-heartbeat.timer -p NextElapseRealtimestamp --value)"
say "  follow logs:               sudo journalctl -u ikrs-heartbeat -f"
say "  fire one tick now:         sudo systemctl start ikrs-heartbeat.service"
say "  uninstall:                 sudo $SCRIPT_DIR/uninstall.sh"
if $NEEDS_TOKEN_MIGRATION; then
  echo ""
  warn "ACTION REQUIRED: migrate legacy google-token.json to Firestore."
  warn "  source $SECRETS_FILE && export TOKEN_ENCRYPTION_KEY TOKEN_ENCRYPTION_KEY_VERSION"
  warn "  sudo -E $VENV_DIR/bin/python $HEARTBEAT_SOURCE/scripts/migrate-token-to-firestore.py <engagement-id>"
fi
