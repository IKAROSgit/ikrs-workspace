#!/usr/bin/env bash
#
# CI + pre-commit guard: if a commit touches architecture-sensitive
# files but does NOT touch docs/ECOSYSTEM.md, fail with a clear message.
#
# Sensitive paths are anything that, when changed, would invalidate
# something documented in ECOSYSTEM.md — runbooks, schema, identity,
# scheduling, secrets handling, phase status.
#
# Usage:
#   bash scripts/check-ecosystem-docs.sh [BASE_REF [HEAD_REF]]
#
# Defaults:
#   BASE_REF = $GITHUB_BASE_REF (CI) or merge-base with origin/main (local)
#   HEAD_REF = HEAD
#
# Exit codes:
#   0 — no sensitive changes, OR sensitive + ECOSYSTEM.md changed
#   1 — sensitive changes without ECOSYSTEM.md update
#   2 — usage / git error

set -euo pipefail

DOC="docs/ECOSYSTEM.md"

# Files / paths that, when modified, REQUIRE a doc update.
# Add to this list when introducing a new sensitive surface.
SENSITIVE_PATTERNS=(
  # Heartbeat (Tier II) — entire package
  "^heartbeat/"
  # Heartbeat (Tier I) — Rust
  "^src-tauri/src/heartbeat/"
  "^src-tauri/src/commands/heartbeat\\.rs"
  # JS reconciler + UI
  "^src/hooks/useHeartbeatTierI\\.ts"
  "^src/components/heartbeat/"
  # Tauri identity / auth / OAuth flows
  "^src-tauri/src/oauth/"
  "^src-tauri/src/commands/oauth\\.rs"
  "^src-tauri/src/commands/credentials\\.rs"
  # Firebase / Firestore config
  "^firestore\\.rules$"
  "^firestore\\.indexes\\.json$"
  "^firebase\\.json$"
  # CI / build / deploy pipeline
  "^\\.github/workflows/"
  "^scripts/"
  "^heartbeat/scripts/"
  "^heartbeat/systemd/"
  # Spec docs (architecture changes are spec changes)
  "^docs/specs/"
  # Tauri configuration
  "^src-tauri/Cargo\\.toml$"
  "^src-tauri/tauri\\.conf\\.json$"
  "^src-tauri/entitlements\\.plist$"
  # Top-level instructions
  "^CLAUDE\\.md$"
  "^AGENTS\\.md$"
)

usage() {
  cat <<EOF
Usage: $0 [BASE_REF [HEAD_REF]]

Without arguments, compares HEAD against the merge-base with origin/main.
On CI, set GITHUB_BASE_REF or pass refs explicitly.

  $0                              # local pre-commit
  $0 origin/main                  # compare HEAD vs main
  $0 origin/main HEAD~1           # compare HEAD~1 vs origin/main
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

# Resolve refs.
BASE="${1:-}"
HEAD="${2:-HEAD}"
if [[ -z "$BASE" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    BASE="origin/${GITHUB_BASE_REF}"
  else
    BASE="$(git merge-base origin/main HEAD 2>/dev/null || echo "")"
    if [[ -z "$BASE" ]]; then
      # Fallback for shallow / first-commit cases.
      BASE="HEAD~1"
    fi
  fi
fi

if ! git rev-parse --verify "$BASE" >/dev/null 2>&1; then
  echo "check-ecosystem-docs: cannot resolve BASE ref '$BASE'." >&2
  exit 2
fi
if ! git rev-parse --verify "$HEAD" >/dev/null 2>&1; then
  echo "check-ecosystem-docs: cannot resolve HEAD ref '$HEAD'." >&2
  exit 2
fi

# Structural self-check: ECOSYSTEM.md MUST contain the
# "Integration coverage checklist" section. This is the anchor that
# prevents new integrations from being added without dedicated doc
# sections (CLAUDE.md rule 3). If it's missing, fail loud — the
# repo's documentation contract is broken.
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
if [[ -f "$REPO_ROOT/$DOC" ]]; then
  if ! grep -Fq "## Integration coverage checklist" "$REPO_ROOT/$DOC"; then
    {
      echo
      echo "❌ check-ecosystem-docs: $DOC is missing the"
      echo "   '## Integration coverage checklist' section."
      echo
      echo "This section is required (CLAUDE.md rule 3). It enumerates"
      echo "every external integration the system uses + the doc section"
      echo "where each is documented. Restore it before this check can pass."
      echo
      echo "If you're trying to remove a deprecated integration, move its"
      echo "row to the 'Removed integrations' subsection instead of"
      echo "deleting the whole checklist."
      echo
    } >&2
    exit 1
  fi
fi

CHANGED="$(git diff --name-only "$BASE" "$HEAD")"
if [[ -z "$CHANGED" ]]; then
  echo "check-ecosystem-docs: no changes between $BASE and $HEAD."
  exit 0
fi

# Did this changeset touch any sensitive path?
SENSITIVE_HITS=()
while IFS= read -r file; do
  for pattern in "${SENSITIVE_PATTERNS[@]}"; do
    if [[ "$file" =~ $pattern ]]; then
      SENSITIVE_HITS+=("$file")
      break
    fi
  done
done <<< "$CHANGED"

# Did it also touch the doc?
DOC_HIT=false
if echo "$CHANGED" | grep -Fxq "$DOC"; then
  DOC_HIT=true
fi

if [[ ${#SENSITIVE_HITS[@]} -eq 0 ]]; then
  echo "check-ecosystem-docs: no sensitive paths changed; skipping doc requirement."
  exit 0
fi

if $DOC_HIT; then
  echo "check-ecosystem-docs: sensitive paths changed AND $DOC was updated. ✓"
  exit 0
fi

# Sensitive without doc — fail with a useful message.
{
  echo
  echo "❌ check-ecosystem-docs: $DOC was NOT updated, but the following"
  echo "   sensitive files changed:"
  echo
  for f in "${SENSITIVE_HITS[@]}"; do
    echo "     - $f"
  done
  echo
  echo "Update $DOC in the same commit / PR. The doc is the canonical"
  echo "reference for architecture, identity, file locations, schema,"
  echo "scheduling, secrets handling, runbooks, and phase status."
  echo
  echo "If your change genuinely doesn't affect any of those:"
  echo "  - Add a 'docs(ecosystem): no-op (touched X but no behaviour change)'"
  echo "    line in $DOC bumping the 'Last verified' note. That counts as"
  echo "    an update and proves you read it."
  echo "  - Or, if the sensitive-pattern list itself is wrong for this case,"
  echo "    update SENSITIVE_PATTERNS in scripts/check-ecosystem-docs.sh"
  echo "    AND $DOC's Update Protocol section in the same commit."
  echo
  echo "See CLAUDE.md for the full rule."
  echo
} >&2
exit 1
