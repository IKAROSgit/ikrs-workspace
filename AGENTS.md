# AI agent instructions for ikrs-workspace

This file mirrors `CLAUDE.md` for tools that look for `AGENTS.md` (Cursor,
Aider, Continue, generic agent harnesses, etc.).

**The full operating rules are in `CLAUDE.md`.** Read that first. The
two non-negotiables, repeated here:

1. **READ `docs/ECOSYSTEM.md`** before any non-trivial change. It is
   the canonical reference for architecture, identity, file locations,
   Firestore schema, runbooks, and phase status.

2. **UPDATE `docs/ECOSYSTEM.md` IN THE SAME COMMIT** as any change that
   touches architecture, secrets handling, Firestore schema, scheduling,
   operator runbooks, phase status, or known limitations.

CI enforces rule 2 via `scripts/check-ecosystem-docs.sh` (see
`.github/workflows/ci.yml`). PRs that change sensitive files without
updating the doc will fail the check.

If you are an AI agent and you ignored these rules, the human will be
right to undo your work and re-instruct you. Don't make them have to.
