# Claude — operating instructions for ikrs-workspace

> **Read this first. Always. Even if you've worked on this repo before
> in another session, the rules below take precedence over your priors.**

## CRITICAL — the canonical doc rule

**`docs/ECOSYSTEM.md` is the single source of truth for this codebase.**

It documents:
- The full architecture (Tauri Mac app, Tier I + Tier II heartbeat, Firebase)
- Identity model (consultant UIDs, client portal UIDs, service accounts)
- What lives where (Mac filesystem, VM filesystem, GitHub, Firestore)
- Phase status (shipped, in-progress, deferred)
- Operational runbooks (install, deploy, debug, rotate secrets)
- Schema reference (Firestore docs, TickState, prompt template)
- Known limitations and open work

**Three non-negotiable rules:**

1. **READ `docs/ECOSYSTEM.md` IN FULL BEFORE making any non-trivial change.**
   Not skim. Read. Stale knowledge has bitten this project before.

2. **UPDATE `docs/ECOSYSTEM.md` IN THE SAME COMMIT** as any change that
   touches:
   - Architecture (new services, file moves, new deps, auth flow changes)
   - Secret material (env vars, keychain entries, token types, paths)
   - Firestore (new collections / fields / rules / indexes)
   - Scheduling (systemd units, tokio intervals, cron)
   - Operator runbooks (install / deploy / rotate steps)
   - Phase status (new phase, phase moved between states)
   - Known limitations (new ones found, old ones fixed)

3. **WHEN ADDING A NEW INTEGRATION**, the integration MUST get its own
   dedicated, comprehensive section in `docs/ECOSYSTEM.md` —
   *why it exists, how it's set up, how it's used, what it does NOT
   do, what's planned next* — AND must be added to the
   "Integration coverage checklist" table near the top of the doc.
   "Mentioned in passing across 3 sections" does NOT satisfy this rule.
   See the checklist for the current list and `⚠️`/`❌` rows
   that document existing technical debt to clear.

CI enforces rule 2 via `scripts/check-ecosystem-docs.sh`. Rule 3 is
enforced by human review at PR time + the checklist section's
self-policing format. Rules 1 and 3 are enforced by your professional
discipline. Don't be the agent that fails them.

If unsure whether a change qualifies, update the doc anyway.

## Workflow

1. **Plan** before code. Even small changes get a one-line plan.
2. **Read first**: `docs/ECOSYSTEM.md`, then any relevant spec in
   `docs/specs/`, then the code itself.
3. **Atomic commits.** One concept per commit. Reference the spec/issue.
4. **Test before push.** Lint + types + tests must pass on the layer you
   touched (Python: ruff + mypy + pytest; Rust: cargo clippy + cargo
   test; TS: tsc --noEmit + vitest).
5. **Adversarial review.** For new architectural surfaces, dispatch a
   pre-code challenge agent before writing AND a post-code challenge
   agent before merge (see Phase E commit history for the pattern).
6. **Commit author email**: use `IKAROSgit@users.noreply.github.com` —
   the operator has GitHub email privacy enabled, real-email commits
   get rejected at push.
7. **No `--no-verify`.** Pre-commit hooks exist for a reason.

## Important constraints

- **Anthropic Consumer Terms**: Claude Code subprocess runs only when the
  human is present (Tier I). Tier II uses Gemini paid tier (commercial-OK).
  Don't add code that calls Claude unattended on the VM.
- **No IKAROS-held API keys.** Operator brings their own (Gemini, Telegram,
  Firebase). Don't bake any centralized secret into the codebase.
- **Single Firebase project posture for now** (`ikaros-portal`). Multi-tenant
  / per-tenant projects deferred to Phase F+.
- **Tauri sandbox is OFF** (`com.apple.security.app-sandbox` removed in
  commit 44a4699). Don't add it back without spec change — the app
  legitimately needs subprocess exec + reads outside the container.

## Stack reminders

- **Mac app**: Tauri 2 (Rust 1.x stable + React 19 + Vite 7 + Tailwind 4).
- **Tier II heartbeat**: Python 3.11+ (3.12 on Mac dev, 3.11 on Debian VM).
  `google-genai>=1.70` (NOT the deprecated `google-generativeai`).
- **State persistence**: Firestore for cross-machine; atomic JSON file
  writes for VM-local TickState.
- **Pip install on VM**: editable install at `/opt/ikrs-heartbeat/venv`.
  Re-run `pip install -e ~/projects/apps/ikrs-workspace/heartbeat` after
  any code pull on the VM.

## When in doubt

Ask the human. State the question + your best guess + the cost of being
wrong. Don't assume. Don't make destructive changes (delete, force-push,
amend pushed commits) without explicit confirmation.
