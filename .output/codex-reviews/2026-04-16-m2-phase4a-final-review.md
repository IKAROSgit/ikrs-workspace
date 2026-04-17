# CODEX REVIEW

**Subject:** M2 Phase 4a — macOS App Sandbox readiness + code signing (retroactive final review)
**Type:** phase-review (security-audit)
**Date:** 2026-04-16
**Reviewed by:** Codex (proxy: superpowers:code-reviewer per CODEX.md fallback)
**Commit range:** `482c90f..01ef2a6` (15 commits, pre-`b4bb80a` Phase 4b)
**Repo HEAD:** `b4bb80a` (main, /home/moe_ikaros_ae/projects/apps/ikrs-workspace)
**Spec:** `docs/specs/m2-phase4a-sandbox-signing-design.md`
**Plan:** `docs/superpowers/plans/2026-04-12-m2-phase4a-sandbox-signing.md`

---

## Context

Codex canvas is offline; CODEX.md explicitly authorises the `superpowers:code-reviewer`
agent as the review mechanism in that case. Prior Phase 4a artefacts:

- Spec: WARN 7/10 addressed (2026-04-12)
- Wave 2 interim reviews (`dafcbc5`) and F2 PATH separator fix (`01ef2a6`) both in-tree
- No retroactive final review had been filed before Phase 4b shipped — this closes the gap.

Full 4a scope = Phase-3 debt cleanup (app identifier, OAuth refresh token, SettingsView
OAuth), sandbox prep (binary resolver, entitlements, capabilities, persisted-scope,
Windows cfg guards), macOS bundle config + CSP, CI signing env wiring, and validation.

Tests verified locally: **58 Rust tests pass** (matches the exit-criteria claim of
58 Rust + 55 frontend).

---

## VERDICTS

**1. Structural:     PASS** — The five-wave decomposition (Phase-3 debt → sandbox prep
→ build config → CI signing → validation) maps 1:1 onto the commits. Binary resolver,
token refresh, entitlements, and CI wiring each live in their own commit with tests.
The module graph is clean: `oauth::token_refresh` consumed by `claude::commands`;
`claude::binary_resolver` managed via `app.manage()` and consumed by `session_manager`
+ `mcp_config`. No circular deps introduced.

**2. Architecture:   PASS** — Matches spec §4.2 (runtime binary resolver with
`which`-first, then known-path fallback, nvm glob included), §5 (entitlements +
restricted capabilities with scoped `shell:allow-execute` for `claude`), §6
(`tauri-plugin-persisted-scope` registered after `fs::init()` in `lib.rs` line 48,
ordering correct), §7 (Unix/Windows cfg guards on `is_process_alive`,
`is_claude_process`, `kill_process`), §8 (macOS bundle: minOS 12.0, entitlements
path, `signingIdentity: "-"`, `public.app-category.business`). App identifier
change to `ae.ikaros.workspace` is reverse-domain per Apple notarisation rules and
matches Golden Rule #1 (ikaros.ae). Data-migration helper in `lib.rs:10-40` is
defensive: `rename` first, file-by-file copy fallback, skips when new dir exists
— tested end-to-end in `tests::test_migrate_app_data_*`.

**3. Security:       PASS (with one WARN-level note)** — This is the critical axis
for Phase 4a and the posture is clean:
  - **No private signing material in the repo.** `find` for `*.p12`, `*.pem`,
    `AuthKey*`, `*.cer` returned zero hits. All six Apple env vars
    (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
    `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) are wired through
    `${{ secrets.* }}` only — `.github/workflows/ci.yml:92-98`. `tauri-action`
    skips signing gracefully when secrets are absent, so PRs from forks do not
    fail.
  - **Refresh-token handling is correct.** `token_refresh.rs` stores the full
    JSON payload (`access_token`, `refresh_token`, `expires_at`, `client_id`) in
    keychain via `tauri-plugin-keyring`. `is_expired()` applies a 5-minute buffer
    and corrupted/legacy plaintext blobs fail parse → graceful
    "re-authenticate" error rather than panic (tested in
    `test_plain_token_string_is_handled` and `test_corrupted_json_is_handled`).
  - **CSP widened conservatively.** Only `http://localhost:*` added to
    `connect-src` for the OAuth redirect server — no wildcard `https:` in
    `connect-src`. `img-src 'self' data: https:` is unchanged (not tightened,
    but not a regression).
  - **Capabilities narrowed.** `shell:allow-execute` is scoped to a named
    `claude` command (not `*`) in `capabilities/default.json:10-12`.
    `keyring:default` is present but that plugin only exposes get/set by
    service/account — not a broad exfiltration vector.
  - **Notarisation flow.** `tauri-action@v0` with `APPLE_ID` + `APPLE_PASSWORD`
    + `APPLE_TEAM_ID` set triggers notarytool + stapling inside the action (this
    is the documented upstream behaviour). This is not empirically validated in
    CI yet because Apple Developer enrolment is blocked on a human task —
    acknowledged both in the spec §13 and the exit criteria (first checkbox
    unchecked).
  - **Pre-existing acknowledged debt.** `keyring:default` allows any frontend
    code to read arbitrary keychain entries for service `ikrs-workspace`. Not a
    Phase 4a regression, and consistent with CLAUDE.md Golden Rule #4 acknowledged
    posture, but worth flagging for M3 hardening.
  - **One WARN:** Entitlement `com.apple.security.cs.allow-unsigned-executable-memory`
    is present (needed for Node.js V8 JIT in npx-spawned MCP servers per spec
    §5.1). The spec explicitly says "test empirically — if not needed, remove
    (tighter sandbox); `allow-jit` is a more restrictive alternative." That
    empirical test is blocked on signed-build availability. **Action:** during
    first signed build, verify whether `allow-jit` is sufficient and downgrade
    if so. This is sandbox tightening, not a leak, hence security remains PASS.

**4. Completeness:   PASS** — All 12 tasks across all 5 waves committed. No
placeholders or TODOs in Phase 4a code. (The `GENERATED_PUBLIC_KEY_HERE` placeholder
in `tauri.conf.json` arrived with Phase 4b commit `b4bb80a`, not Phase 4a — confirmed
via `git show 24ddbce`.) Exit criteria honestly track status: 7/8 ticked, the last
("Signed .dmg installs without Gatekeeper warning") correctly marked blocked on
Apple Developer enrolment.

**5. Risk register:  PASS** — All eight P4-R1..R8 risks in the spec have landed
mitigations:
  - R1 (Claude not found): binary resolver + `resolve_binary("claude", &candidates)`
    + graceful setup-hook warning.
  - R2 (npx subprocess chain): absolute npx path threaded through `mcp_config.rs`
    + PATH injection into Claude CLI via `session_manager.rs:92-103`.
  - R3 (bookmark revoked): `tauri-plugin-persisted-scope` registered after
    `tauri_plugin_fs::init()` — correct ordering per spec §6.2.
  - R4 (shell:execute rejection): scoped to named `claude` command.
  - R5 (OAuth redirect): `network.server` entitlement in plist.
  - R6 (unsigned dylibs): delegated to tauri-action; pending first notarised
    build.
  - R7 (Apple enrolment delay): all non-signing work shipped independently — as
    designed.
  - R8 (V8 JIT): `allow-unsigned-executable-memory` set; `allow-jit` downgrade
    deferred — captured in WARN above.

  New risk introduced: **R9 — ad-hoc signing in dev (`signingIdentity: "-"`).**
  Fine for local dev, but must be replaced by the real Developer-ID identity in
  CI via env var. CI workflow does not currently override `signingIdentity`
  explicitly — `tauri-action` handles this when `APPLE_SIGNING_IDENTITY` is
  present. Document this assumption in the Phase 4 handoff to avoid a future
  "why is our dmg still ad-hoc signed" regression.

**6. Spec alignment: PASS** — Spec §3.2 (token payload schema) → matches
`TokenPayload` struct byte-for-byte. Spec §4.2 resolution order preserved
(`which` → `~/.claude/local/bin/claude` → `/usr/local/bin` → `/opt/homebrew/bin`).
Spec §5.2 capability JSON matches `capabilities/default.json` exactly (as of
commit `01ef2a6`; Phase 4b later added `updater:default` — out of scope).
Spec status updated in commit `2744f95` to reflect implementation complete,
per Golden Rule #12.

  One deviation, justified: the Wave-2 review surfaced that unconditional `:`
  PATH separator would break Windows. Fix `01ef2a6` introduces
  `cfg!(target_family = "windows")` in `session_manager.rs:101` and
  `binary_resolver.rs:31`. This is an improvement over the spec, not a regression.

**7. Readiness:      PASS** — 58 Rust tests pass locally (confirmed in this
review). The binary resolver has 5 tests covering path-env assembly,
de-duplication, and None handling. Token refresh has 5 tests covering happy
path, expired-within-buffer, corrupted JSON, and pre-Phase-4a plaintext format.
Migration has 3 tests. `cargo check` passes via test-compile. No frontend
breakage from `SettingsView.tsx` migration to `startOAuthFlow` — tauri-commands
exports cleaned up in `9414b98`. CI workflow updated with signing env vars +
per-OS artefact upload with `if-no-files-found: ignore` — correct posture for
matrix runs where only one OS produces each bundle type.

---

## Findings (actionable)

### WARN — address during M2→M3 transition, not blocking

**W1. `allow-unsigned-executable-memory` vs `allow-jit`.** Downgrade to the
narrower entitlement during first notarised build if Claude CLI + npx-spawned
MCP servers tolerate it. Spec §5.1 and Risk P4-R8 both anticipate this; this
review just re-states the TODO so it is not lost.

**W2. Keychain service scope (pre-existing debt).** Any frontend code can read
any keychain entry in service `ikrs-workspace` via the `keyring:default`
capability. Not a Phase 4a regression — this is how Phase 3 already worked —
but M3 should scope keyring to specific accounts or expose only purpose-built
Rust commands.

**W3. Documentation debt.** The Phase 4a session handoff doc in `.output/` is
not present (only the Phase-1 handoff from 2026-04-11 exists). Golden Rule #12
checklist has one unchecked box. **Recommended:** have the next session write
`2026-04-13-m2-phase4a-session-handoff.md` describing the Phase-3-debt cleanup
and the sandbox-prep posture, so Phase 4b (already shipped at `b4bb80a`) has
an explicit trailing artefact. Downgrading from FAIL to WARN because the spec
itself was updated in `2744f95` and the plan tracks status.

### Suggestions (nice-to-have)

**S1.** `binary_resolver.rs::resolve_binary` silently drops the stderr from
`which`. On a machine where `which` is present but fails (e.g., PATH entirely
empty under sandbox), we would never know why. A single-line `log::debug!` on
non-success would aid field diagnostics.

**S2.** The `TokenPayload.refresh_token` is stored as `String` with an
`unwrap_or("")` fallback in `redirect_server.rs:109`. If Google ever omits
`refresh_token` (it does on repeat `prompt=consent` flows), `refresh_if_needed`
will POST with an empty string and 400. Consider `Option<String>` and a clearer
error: "This Google session has no refresh token; please re-authenticate with
`prompt=consent`."

**S3.** CI artefact upload globs include `nsis/*.exe` but the Tauri config
targets `"all"` which on Windows defaults to MSI. Unused `nsis/*.exe` line is
harmless (`if-no-files-found: ignore`), just dead code.

---

## Golden Rules Check

- **GR-1 (one repo, one truth):** PASS. App identifier reverse-domain
  (`ae.ikaros.workspace`) aligns with ikaros.ae.
- **GR-3 (no stubs):** PASS. Data migration, PATH resolution, token refresh all
  complete — no TODOs in the 4a code.
- **GR-4 (secrets posture):** PASS for this phase. All Apple secrets are in
  GitHub Secrets. No `.p12` / `.pem` in repo.
- **GR-5 (security not optional):** PASS. Sandbox entitlements are explicit and
  minimal, capabilities are scoped, CSP widened only for localhost.
- **GR-12 (documentation current):** PASS (WARN). Spec status updated
  (`2744f95`), plan tracks waves, but a dedicated session-handoff doc is
  missing — flagged as W3.
- **Mistake #4 (IKAROS identity):** PASS. `productName: "IKAROS Workspace"`,
  `ae.ikaros.workspace` identifier, `Developer ID Application: IKAROS FZ-LLC`
  documented in spec §9.1.

---

## DECISION: **APPROVED WITH CONDITIONS**

**Score: 8.5/10**

Strong execution on a security-critical phase. The refresh-token story is the
standout — Rust-side refresh with a 5-minute buffer, corruption-tolerant
parsing, and client-id embedded in the keychain blob is a clean design choice
that keeps the frontend contract stable. Binary resolver + PATH injection
solves the hardest sandbox problem (npx/node under restricted PATH) without
bundling node. No private signing material in the repo. Test coverage for the
new modules is proportionate (10 new tests on 303 new lines).

The 1.5-point deduction:
- −0.5 W1 (entitlement not yet tightened to `allow-jit`; empirical test
  blocked on signed build).
- −0.5 W3 (missing Phase 4a session handoff doc — Golden Rule #12 partial
  miss).
- −0.5 because notarisation path is **designed but not yet empirically
  verified end-to-end** (Apple Developer enrolment is the blocking human task
  called out in spec §13). Until a signed+stapled dmg has actually installed
  on a clean macOS 12+ machine, Phase 4a cannot claim full completion. This is
  a known and acknowledged gap, not a defect.

### Conditions (must close within M2)

1. **[must]** Write `.output/2026-04-13-m2-phase4a-session-handoff.md`
   summarising Phase-3 debt cleanup, sandbox-prep posture, and the exact list
   of GitHub Secrets required once Apple enrolment completes.
2. **[must]** First signed CI build: verify notarytool + stapling produce a
   Gatekeeper-clean dmg. Capture the resulting artefact size + notarisation
   ticket UUID in the handoff.
3. **[should]** Evaluate whether `allow-jit` is sufficient in place of
   `allow-unsigned-executable-memory`. Downgrade if so.
4. **[should]** `TokenPayload.refresh_token` → `Option<String>` with a clearer
   error path when missing (S2).

### Not blocking Phase 4b

Phase 4b (commit `b4bb80a`) has already shipped. This retroactive review
confirms that the foundation it builds on is sound. Recommend the above
conditions be tracked in the M2 milestone audit rather than re-opening 4a.
