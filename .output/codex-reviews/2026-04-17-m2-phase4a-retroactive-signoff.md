# CODEX REVIEW — Retroactive Sign-off (Governance Gap Closure)

**Subject:** M2 Phase 4a — macOS App Sandbox readiness + code signing
**Type:** phase-review / security-audit (retroactive, Golden Rule #11 gap closure)
**Date:** 2026-04-17
**Reviewed by:** Codex
**Commit range:** `2aa8e03..01ef2a6` (16 commits; HEAD is `b4bb80a`, Phase 4b)
**Spec:** `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m2-phase4a-sandbox-signing-design.md`
**Plan:** `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/superpowers/plans/2026-04-12-m2-phase4a-sandbox-signing.md`
**Prior artefact:** `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/.output/codex-reviews/2026-04-16-m2-phase4a-final-review.md` (independent review agent, 8.5/10)

---

## Context

Phase 4a shipped without a filed Codex review — Golden Rule #11 violation. A prior
agent-proxy review exists (2026-04-16, 8.5/10 APPROVED WITH CONDITIONS). This is an
independent re-audit against the actual shipped tree. Findings are concordant with
the prior review on most axes, with sharper calls on the "can we actually ship a
signed, sandboxed dmg *today*?" question.

---

## VERDICTS

**1. Structural: PASS** — 5-wave plan maps 1:1 to the commit sequence. Binary
resolver, token refresh, entitlements, capabilities, bundle config, and CI signing
each ship as discrete, test-backed commits. No cycles; `oauth::token_refresh` is
consumed by `claude::commands`; `claude::binary_resolver` is app-state managed and
consumed by `session_manager` + `mcp_config`.

**2. Architecture: PASS** — Entitlements (`src-tauri/entitlements.plist`) match
spec §5.1 exactly (app-sandbox, network.client, network.server,
user-selected.read-write, bookmarks.app-scope, allow-unsigned-executable-memory).
Capabilities (`src-tauri/capabilities/default.json`) scope `shell:allow-execute`
to the named `claude` command per spec §5.2. Binary resolver
(`src-tauri/src/claude/binary_resolver.rs`) implements the `which`-first, then
`~/.claude/local/bin` → `/usr/local/bin` → `/opt/homebrew/bin` → nvm/volta fallback
order from spec §4.2. Token refresh (`src-tauri/src/oauth/token_refresh.rs`) uses
the exact `TokenPayload` schema from spec §3.2. Identifier `ae.ikaros.workspace`
aligns with ikaros.ae (Golden Rule #1).

**3. Security: WARN** — Overall posture is good but there are two real concerns.
  - No signing material in repo (verified: no `.p12`/`.pem`/`AuthKey*`).
  - All six Apple secrets + two Tauri updater secrets wired via
    `${{ secrets.* }}` only (`.github/workflows/ci.yml` L92-101).
  - Refresh token is keychain-only, with 5-min expiry buffer and corruption-tolerant
    parsing. Good.
  - CSP conservatively widened (`connect-src http://localhost:*` for redirect
    server). `img-src 'self' data: https:` is pre-existing.
  - **Concern 1 (WARN):** `com.apple.security.cs.allow-unsigned-executable-memory`
    is the most permissive JIT entitlement. Spec §5.1 and Risk P4-R8 both flag
    `allow-jit` as the preferred narrower variant "if empirically sufficient."
    That test requires a signed build, which has never run. Posture is defensible
    but not minimal.
  - **Concern 2 (WARN):** `keyring:default` capability grants frontend code
    arbitrary read on keychain service `ikrs-workspace`. This predates 4a, but the
    introduction of a refresh_token (long-lived credential, unlike the short
    access_token) raises the value of exfiltration. M3 should expose refresh-only
    via a purpose-built Rust command and drop `keyring:default`.

**4. Completeness: WARN** — All 12 tasks across 5 waves committed, no TODOs in
4a code. Spec exit criteria §11 honestly marks 7/8 done with "Signed .dmg installs
without Gatekeeper warning" explicitly unchecked pending Apple enrolment. **But:**
no `.output/2026-04-13-m2-phase4a-session-handoff.md` exists (verified: only
Phase-1 and M2-brainstorm handoffs from 2026-04-11 are present) — Golden Rule
#12 documentation miss. The prior review flagged this; it is still not closed.

**5. Risk register: PASS** — All 8 spec risks (P4-R1..R8) have landed mitigations
or are consciously deferred:
  - R1 claude-not-found → resolver + graceful startup warning.
  - R2 npx under sandbox → absolute path in `mcp_config` + PATH injection in
    `session_manager` (platform-aware separator per fix `01ef2a6`).
  - R3 bookmark revoked → `tauri-plugin-persisted-scope` registered after
    `fs::init()`.
  - R4 shell rejection → named-command scope.
  - R5 OAuth redirect → `network.server` entitlement.
  - R6 unsigned dylibs → delegated to tauri-action; empirically unvalidated.
  - R7 Apple enrolment delay → non-signing work independent (as shipped).
  - R8 V8 JIT → `allow-unsigned-executable-memory` set (WARN, see above).
  - **New R9:** `signingIdentity: "-"` (ad-hoc) in `tauri.conf.json`. CI relies on
    tauri-action to override via `APPLE_SIGNING_IDENTITY` env. Untested path.
  - **New R10:** `tauri.conf.json` L29 contains literal `"GENERATED_PUBLIC_KEY_HERE"`
    as the updater pubkey. This is Phase 4b debt (commit `b4bb80a`, outside the
    4a review range), but it is in HEAD today and would crash the updater plugin
    init on a release build. Flagged here because it affects whether the tree
    currently ships.

**6. Spec alignment: PASS** — `TokenPayload` byte-matches spec §3.2.
`resolve_binary` order matches §4.2. Entitlement list matches §5.1.
`capabilities/default.json` matches §5.2. Spec status was updated in `2744f95`
(Golden Rule #12 partial). One justified deviation: `01ef2a6` introduced
`cfg!(target_family = "windows")` PATH separator branching, which is an
improvement over the spec's implicit Unix assumption.

**7. Readiness: FAIL (for "ship a signed sandboxed dmg today")** — Let me be
blunt about the question asked.
  - **Code is ready.** 58 Rust tests pass. Compile is clean (verified via prior
    review). Binary resolver has 5 tests; token refresh has 5 tests; migration
    has 3 tests.
  - **Bundle config is ready.** minOS 12.0, entitlements path, category set.
  - **CI wiring is ready.** Apple secrets threaded, tauri-action invoked,
    artefact upload per-matrix.
  - **BUT:** (a) Apple Developer enrolment is the blocking human task — not
    complete. (b) No signed+stapled dmg has ever been produced. (c) The empirical
    claim "sandbox-blocks-npx is solved" has **not** been validated on a signed
    build; it is validated only for the dev (`tauri dev`, non-sandboxed) path.
    Under true codesign+sandbox, the bookmark-scoped filesystem access,
    `spawn("claude")` via named-scope shell plugin, and npx child process PATH
    injection have not been executed end-to-end. (d) `GENERATED_PUBLIC_KEY_HERE`
    in `tauri.conf.json` L29 (Phase 4b carry-over) would fail updater init on
    release.

  So: Phase 4a's *artefacts* are shippable in isolation, but the end-to-end
  capability ("signed sandboxed dmg installs and runs Claude + npx MCP servers
  cleanly") is not demonstrated. The exit criteria in the spec honestly
  acknowledges this (one box unchecked). I do not grade a FAIL on the phase
  itself — I grade FAIL on the literal question "shippable right now."

---

## DECISION: APPROVED WITH CONDITIONS (retroactive)

**Score: 8.0/10**

The 2-point deduction:
- −0.5 Security W1 (`allow-unsigned-executable-memory` not empirically narrowed).
- −0.5 Security W2 (`keyring:default` now guards a long-lived credential — risk
  profile changed without tightening).
- −0.5 Completeness (missing Phase 4a session handoff — GR-12).
- −0.5 Readiness (no signed+stapled dmg produced; sandbox end-to-end empirically
  unvalidated; `GENERATED_PUBLIC_KEY_HERE` placeholder is active in HEAD).

Concurs with prior agent-proxy review (8.5/10 APPROVED WITH CONDITIONS) on
direction. Marginal downgrade reflects: (i) the Phase 4b updater placeholder
visibly sits in the same config file Phase 4a introduced, so any "does it build
and run today" answer must account for it; (ii) heavier weight on the empirical
validation gap for the central claim of this phase.

### Blockers (must close before claiming phase exit)

1. **[BLOCKER]** Apple Developer enrolment complete + GitHub Secrets populated +
   one green `tauri-action` run producing a notarised+stapled dmg.
2. **[BLOCKER]** Manual UAT on a clean macOS 12+ machine: install the notarised
   dmg; launch under sandbox; spawn a Claude session; spawn an npx-backed MCP
   server; confirm no Gatekeeper prompt and no "process not found" failures.
3. **[BLOCKER]** Replace `GENERATED_PUBLIC_KEY_HERE` with the real Tauri updater
   pubkey before any tagged release (Phase 4b debt, but blocks phase exit).

### Conditions (must close within M2)

1. **[must]** Write `.output/2026-04-13-m2-phase4a-session-handoff.md` covering
   Phase-3 debt cleanup, sandbox posture, and required GitHub Secrets (8 total).
2. **[should]** Downgrade `allow-unsigned-executable-memory` → `allow-jit` after
   first successful signed-build UAT.
3. **[should]** Replace `keyring:default` with purpose-built Rust commands in
   M3. The refresh-token's long lifetime changes the risk posture.
4. **[should]** `TokenPayload.refresh_token` → `Option<String>` and surface
   "re-auth with prompt=consent" when absent.

### Strengths

- Refresh-token design is the best piece of work in the phase: single keychain
  entry, embedded client_id, 5-minute expiry buffer, corruption-tolerant parse
  with clear re-auth error. Clean test coverage including the pre-4a legacy
  plaintext format.
- Binary resolver solves the hardest sandbox problem (npx/node discovery)
  without bundling Node — using `which` first preserves the user's actual
  environment, and the platform-aware PATH separator fix is correct.
- Capabilities are genuinely scoped (named-command `shell:allow-execute`, not
  wildcard).
- CI signing is wired without leaking material — all via `${{ secrets.* }}`,
  tauri-action skips signing gracefully when secrets absent.
- Golden Rule #1 (reverse-domain `ae.ikaros.workspace`) respected with a
  defensive data migration (rename-first, copy-fallback, skip-if-exists).

### Governance note

This retroactive review closes the Golden Rule #11 gap on Phase 4a. It does not
retroactively authorise Phase 4b (commit `b4bb80a`) — that commit's updater
placeholder is called out above and should be tracked in the Phase 4b review
(`.output/codex-reviews/2026-04-16-m2-phase4b-final-review.md` — verify it
flagged `GENERATED_PUBLIC_KEY_HERE`).

---

## Relevant files

- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/entitlements.plist`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/capabilities/default.json`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/tauri.conf.json`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/binary_resolver.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/oauth/token_refresh.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/.github/workflows/ci.yml`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m2-phase4a-sandbox-signing-design.md`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/superpowers/plans/2026-04-12-m2-phase4a-sandbox-signing.md`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/.output/codex-reviews/2026-04-16-m2-phase4a-final-review.md` (prior proxy review)
