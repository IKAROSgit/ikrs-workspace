#!/usr/bin/env bash
#
# Right-Hand Daily Session — wrapper script for launchd.
#
# Invoked by com.ikaros.right-hand.plist at 04:00 Dubai time daily.
# The plist calls this script instead of claude directly so we can
# evolve the invocation (env vars, flags, error handling) without
# re-loading the plist.
#
# Exit codes:
#   0 — session completed (or kill-switch active)
#   1 — claude not found or session error
#   2 — prompt file missing

set -euo pipefail

# ---------- Config ----------

PROJECT_ROOT="$HOME/projects/apps/ikrs-workspace"
VAULT_ROOT="$HOME/.ikrs-workspace/vaults/blr-world-com"
PROMPT_FILE="$PROJECT_ROOT/operations/right-hand/daily-prompt.md"
CLAUDE_BIN="$HOME/.local/bin/claude"
LOG_DIR="$HOME/Library/Logs/ikaros-right-hand"
DATE=$(date +%Y-%m-%d)

# ---------- Pre-flight ----------

mkdir -p "$LOG_DIR"

if [[ ! -x "$CLAUDE_BIN" ]]; then
  echo "[right-hand] ERROR: claude not found at $CLAUDE_BIN" >&2
  exit 1
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "[right-hand] ERROR: prompt file not found at $PROMPT_FILE" >&2
  exit 2
fi

# ---------- Run ----------

echo "[right-hand] starting daily session at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "[right-hand] vault: $VAULT_ROOT"
echo "[right-hand] prompt: $PROMPT_FILE"

cd "$VAULT_ROOT"

# Pass prompt via command substitution (quoted to prevent word-splitting).
# --allowedTools: filesystem + MCP tools only, no Bash.
# Disable set -e around the claude call — non-zero exit (e.g., MCP
# graceful-degradation warnings) should not abort the wrapper before
# the post-run logging below.
set +e
"$CLAUDE_BIN" -p "$(cat "$PROMPT_FILE")" \
  --allowedTools "Read,Write,Edit,Glob,Grep,mcp__*" \
  --output-format text \
  --max-turns 50 \
  2>>"$LOG_DIR/stderr-$DATE.log" \
  >>"$LOG_DIR/stdout-$DATE.log"
EXIT_CODE=$?
set -e

echo "[right-hand] session complete at $(date -u +%Y-%m-%dT%H:%M:%SZ) — exit code $EXIT_CODE"

exit $EXIT_CODE
