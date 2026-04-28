#!/usr/bin/env bash
#
# Install a Git pre-commit hook that runs check-ecosystem-docs.sh
# against the staged changes. Run once after `git clone`:
#
#   bash scripts/install-pre-commit-hook.sh
#
# Re-run safely; it overwrites the existing pre-commit hook.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOOK="$REPO_ROOT/.git/hooks/pre-commit"

if [[ ! -d "$REPO_ROOT/.git" ]]; then
  echo "install-pre-commit-hook: not in a Git repo (no .git/ at $REPO_ROOT)." >&2
  exit 1
fi

cat > "$HOOK" <<'EOF'
#!/usr/bin/env bash
# Auto-installed by scripts/install-pre-commit-hook.sh — runs the
# ECOSYSTEM.md guard against staged changes.
set -euo pipefail
REPO_ROOT="$(git rev-parse --show-toplevel)"
exec "$REPO_ROOT/scripts/check-ecosystem-docs.sh" HEAD
EOF
chmod +x "$HOOK"

echo "✓ Pre-commit hook installed at $HOOK"
echo "  It will run check-ecosystem-docs.sh on every commit."
echo "  Bypass (rare, justified) with: git commit --no-verify"
