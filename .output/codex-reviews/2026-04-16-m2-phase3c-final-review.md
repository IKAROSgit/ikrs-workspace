# Codex Review — M2 Phase 3c (Final, Retroactive)

> Retroactive audit written 2026-04-16. Phase 3c shipped on `main` between
> 0f87341 (spec, 2026-04-12) and 7ccb355 (spec marked Complete, 2026-04-13),
> with follow-on polish landing through b89e820 (2026-04-15). No review
> artifact was written at the time. This review covers the 22-commit range
> and cross-checks current HEAD (b4bb80a) to confirm no 3c regressions.

```
CODEX REVIEW
============
Subject: M2 Phase 3c — MCP Polish, Re-Auth Fix, Strict Mode, Tests
Type: phase-review
Date: 2026-04-16
Reviewed by: Codex (via superpowers:code-reviewer per CODEX.md §"How to Request a Codex Review")

VERDICTS
--------
1. Structural:     PASS  — New modules respect layer boundaries. `src/lib/mcp-utils.ts` is a pure helper (no Tauri imports), `src-tauri/src/oauth/redirect_server.rs` is self-contained behind `oauth::mod`, `binary_resolver.rs` is under `claude::` and managed via `app.manage()` at startup. DAG clean; no cross-store coupling leaked into store internals (Codex N2 honored — mcpStore is cleared from the event-wiring layer, not `claudeStore.setDisconnected`).
2. Architecture:  PASS  — All four design pillars landed: (a) mcpStore populated from `system.init` tools list via `extractMcpServers`; (b) two-step re-auth with one-shot redirect server replaces fire-and-forget browser open; (c) `strict_mcp: Option<bool>` guard in `spawn_claude_session` with resume-bypass per Codex I2; (d) Vitest test suite operational (12 test files, 71 tests passing). Binary resolver wires real paths into both `session_manager.spawn` and `mcp_config::generate_mcp_config` — npx is eliminated at *config time* (`command: "/absolute/path/to/npx"`) AND at runtime (Claude CLI child inherits PATH with resolved dirs prepended).
3. Security:       PASS  — `.mcp-config.json` and `.mcp-config.json.tmp` are in `.gitignore` (lines 23-24). No secret leaks in config: it contains only the `${GOOGLE_ACCESS_TOKEN}` placeholder; real token flows through `.envs(&env_vars)` into Claude subprocess only. Redirect server binds `127.0.0.1` only (never 0.0.0.0), one-shot (accepts exactly one connection then drops), and stores the FULL token payload including `refresh_token` as a JSON blob in OS keychain — an architectural improvement over 3b (which only stored the access token). OAuth PKCE with Desktop client (no client_secret) is correct for Tauri per Google's guidance.
4. Completeness:   PASS — All 10 plan tasks landed with a 1:1 commit map (Task 1=44e3690, Task 2=29b47a4, Task 3=ead4fb3, Task 4=1b26737, Task 5=26dbb71, Task 6=7715fe8, Task 7=05670d0, Task 8=9cbf647+8d553d6, Task 9=02c708b, Task 10=7ccb355). No TODOs, placeholders, or `any` types in the diff. Tests for all four scope items present (mcp-utils.test, mcpStore.test, claudeStore-auth.test, stream_parser auth-error tests, redirect_server::extract_code tests).
5. Risk register:  WARN  — Three residual issues worth tracking (non-blocking, see Findings below): (a) `setTimeout(…, 5*60*1000)` in ChatView's re-auth handler still fires AFTER a successful reauth, calling `cancelOAuthFlow()` on an already-completed flow (harmless abort of a finished handle, but noisy); (b) `cancel_oauth_flow` is not called when `start_oauth_flow` fails mid-setup, leaving a stale `pending_verifier` until the next attempt; (c) `claude:session-crashed` listener does NOT clear mcpStore, so a crashed session leaves stale `isConnected: true` server entries visible to consumer hooks.
6. Spec alignment: WARN  — Spec compliance is high, BUT Golden Rule #12 is partially violated: the parent `embedded-claude-architecture.md` was NOT updated to reflect the two 3c-era architectural additions — strict MCP mode (user-visible, NDA-critical behavior) and the binary resolver (app-startup-time system state). The Phase 3c spec itself is marked "Complete" (7ccb355). Phase 4a's spec references `binary_resolver` but only as a 4a artifact; 3c's own spec doesn't mention it because it landed in 3c's tail (5580ed2, b89e820) as Phase 4a prep. That cross-phase boundary is unclear in the docs.
7. Readiness:      PASS  — Frontend test infra (`tests/setup.ts` with Tauri mocks for `@tauri-apps/api/core`, `@tauri-apps/api/event`, `@tauri-apps/plugin-opener`) is correctly factored. `vitest run` → 12 files, 71 tests, 0 failures. Rust cargo compiles on main (verified via `cargo check` earlier in session). Auth-error inference via `tool_name_map` resolves the latent 3b bug (C2 carry-forward). Re-auth UX improvement ("Waiting for sign-in…") gives the user feedback during the async gap.

DECISION: APPROVED WITH CONDITIONS
Score: 8.5/10
Conditions:
  1. (MUST FIX IN PHASE 4) Fix the ChatView re-auth `setTimeout` leak: clear the timer inside the `oauth:token-stored` listener callback so a successful reauth does not trigger a spurious `cancelOAuthFlow` five minutes later.
  2. (MUST FIX IN PHASE 4) Clear mcpStore on `claude:session-crashed` in `useClaudeStream.ts` symmetrically with `claude:session-ended`.
  3. (SHOULD FIX) Amend `docs/specs/embedded-claude-architecture.md` to document (a) strict MCP mode as an engagement setting and (b) binary resolver as the Phase 3c→4a bridge. Golden Rule #12.
  4. (NICE-TO-HAVE) `start_oauth_flow` should `.abort()` and clear `pending_verifier` inside its own error paths, not rely on the next caller to cancel stale state.
```

---

## Phase 3c Commit Range Audit

Verified by `git log 0f87341^..b89e820`:

| # | Commit | Intent | Evidence |
|---|--------|--------|----------|
| 1 | 0f87341 | Spec created | `docs/specs/m2-phase3c-mcp-polish-design.md` +307 lines |
| 2 | fc83e30 | Spec addresses Codex WARN 7/10 (C1/C2/I1-I4/N1-N4) | +171 lines; C1 (Desktop OAuth), C2 (tool_name_map), I2 (resume bypass), I4 (port fallback) all annotated inline |
| 3 | 03edce2 | Plan | 10 tasks in 4 waves |
| 4 | 44e3690 | Task 1 — `extractMcpServers` + tests | `src/lib/mcp-utils.ts` (25 lines), `tests/setup.ts` (14 lines), 6 unit tests |
| 5 | 29b47a4 | Task 2 — mcpStore tests | 4 tests covering setServers, setServerHealth, empty clear, find-by-type |
| 6 | ead4fb3 | Task 3 — claudeStore auth-error tests | 4 tests; includes "setDisconnected preserves authError" regression guard |
| 7 | 26dbb71 | Task 5 — tool_name_map for auth-error server inference (Codex C2) | `stream_parser.rs` — HashMap threaded through `parse_stream → handle_line → handle_assistant_event/handle_user_event`; `infer_mcp_server` rewritten to match `mcp__{server}__*` prefix |
| 8 | 1b26737 | Task 4 — wire session-ready to mcpStore, clear on disconnect | `useClaudeStream.ts:35-36` adds setServers; `:89` clears on session-ended |
| 9 | 7715fe8 | Task 6 — redirect capture server with port fallback (Codex I4) | `oauth/redirect_server.rs` +147 lines, 3 extract_code tests |
| 10 | 05670d0 | Task 7 — start_oauth_flow + cancel_oauth_flow | `commands/oauth.rs` adds 92 lines; OAuthState extended with `pending_server` |
| 11 | 9cbf647 | Task 8 — two-step re-auth in ChatView | `ChatView.tsx` listens for `oauth:token-stored`, 5-min cancel timeout |
| 12 | 02c708b | Task 9 — strict MCP mode | `commands.rs:26-28` guard; `useWorkspaceSession.ts` 4 call sites updated; `Engagement.settings.strictMcp?` |
| 13 | 66dfb10 | `.gitignore` additions | `.mcp-config.json` + `.mcp-config.json.tmp` |
| 14 | 7ccb355 | Spec marked Complete | Documentation enforcement |
| (post-3c, Phase 4a prep on same branch) | 2aa8e03, 9bec395, b2af00e, 482c90f, c7ac964, 76a8b26, 8d553d6, 9414b98, 5580ed2, b89e820 | Phase 4a spec + identifier rename + token_refresh + binary resolver + SettingsView migration + dead code cleanup | See Phase 4a review when written |

**Plan-to-commit ratio: 10 tasks → 10 plan-aligned commits. Perfect fidelity on the 3c-scoped work.**

---

## 7-Point Detailed Findings

### 1. Structural (PASS)

- `src/lib/mcp-utils.ts` imports only `@/types` (no Tauri). Pure function, easy to test.
- `redirect_server.rs` imports `tauri`, `tauri_plugin_keyring`, `tokio`, `reqwest`, `urlencoding`, `chrono`, `serde_json`, `crate::oauth::token_refresh` — all appropriate for its layer.
- `binary_resolver.rs` uses `std::process::Command` for `which`, `dirs` crate for home, `glob` for nvm scanning. Isolated; no circular deps.
- **Cross-store coupling:** `useClaudeStream.ts` (event-wiring layer) calls `useMcpStore.getState().setServers(…)` — this is the correct location per Codex N2. `claudeStore.setDisconnected` does NOT touch mcpStore. Verified by `grep -n useMcpStore src/stores/` → no hits; stores stay decoupled.

### 2. Architecture (PASS)

**Strict MCP mode implementation** (`commands.rs:26-28`):
```rust
if resume_session_id.is_none() && strict_mcp.unwrap_or(false) && !has_token {
    return Err("Strict MCP mode: Google authentication required. …");
}
```
Correct semantics:
- `resume_session_id.is_none()` → skip check on resume (Codex I2; resumed sessions already have MCP context)
- `strict_mcp.unwrap_or(false)` → opt-in (safe default)
- `!has_token` → token_refresh::refresh_if_needed already ran; expired-and-unrefreshable = no token

**Per-engagement configurability per spec (✓):** `Engagement.settings.strictMcp?: boolean` in `src/types/index.ts:54`. All 4 `spawnClaudeSession` call sites in `useWorkspaceSession.ts` pass `engagement.settings.strictMcp` (Codex N3 honored).

**Can a user get stuck?** Only if (a) their engagement has `strictMcp: true` AND (b) Google token is missing/unrefreshable. Error message is actionable ("Please authenticate before starting this session."). User can either fix auth or toggle `strictMcp` in engagement settings. **No dead-end.** However — the UI does NOT currently expose a toggle for this setting (I checked `SettingsView.tsx` and engagement-editing UIs via grep — zero references to `strictMcp` outside types + hooks). This is acceptable for phase 3c since spec Section 3 scopes it as "engagement setting" without requiring UI; but a future follow-up should add UI or it becomes dead-config.

**Binary resolver eliminates npx at runtime?** Verified — YES, at BOTH layers:
- **Config time** (`mcp_config.rs:29-31`): `npx_path.map(…).unwrap_or_else(|| "npx".to_string())`. When the resolver finds npx at `/opt/homebrew/bin/npx`, that absolute path becomes the `"command"` field in `.mcp-config.json`. The Claude CLI MCP spawner doesn't need PATH resolution for npx.
- **Runtime** (`session_manager.rs:92-110`): `.env("PATH", full_path)` prepends resolved binary directories to the inherited PATH, so transitive resolution (e.g., npx needs to find node) works even under restricted sandbox PATH.

Phase 4a macOS sandbox depends on this: confirmed adequate for sandbox entitlement scope.

### 3. Security (PASS)

- `.mcp-config.json` in `.gitignore` — verified at `.gitignore:23-24`.
- No secrets on disk in any new code path. `.mcp-config.json` uses the literal string `"${GOOGLE_ACCESS_TOKEN}"` as a placeholder; the actual token enters the Claude CLI child process via `Command.envs(&env_vars)` which does NOT persist anywhere.
- Redirect server binds ONLY `127.0.0.1:{port}` (never 0.0.0.0), handles exactly one request then drops the listener. Port scan is 10 ports bounded (preferred_port..=preferred_port+10).
- Keychain storage is a full JSON payload including `refresh_token`, keyed at `ikrs:{engagement_id}:google` via `tauri_plugin_keyring`. OS-level protection (macOS Keychain / Linux libsecret / Windows Credential Manager).
- **PKCE with no client_secret** — correct for Google Desktop OAuth apps. Confirmed via `commands/oauth.rs` + `redirect_server.rs:82-92`: POST to `oauth2.googleapis.com/token` with `code`, `client_id`, `redirect_uri`, `grant_type=authorization_code`, `code_verifier` — no `client_secret` field.
- **Concern flagged but not blocking:** `OAuthState.pending_verifier` uses `std::sync::Mutex` (not `tokio::sync::Mutex`) — this blocks the async executor if contention happens. Real contention is vanishingly unlikely (only one reauth at a time in this UI), but worth noting. Also: if `start_oauth_flow` errors AFTER storing the verifier but BEFORE storing the server handle, the verifier is orphaned (never cleared). Next `start_oauth_flow` call overwrites it, so not an exploit — but untidy.

### 4. Completeness (PASS)

- All 10 plan tasks shipped. No TODO/FIXME added (`grep -rn "TODO\|FIXME" src/ src-tauri/src/ | grep -v node_modules` returns only pre-existing, non-3c items).
- No `any` types introduced. `strictMcp?: boolean` is strongly typed.
- Test coverage for the four declared success-criteria items: `extractMcpServers` (6 tests), `mcpStore` (4 tests), `claudeStore.authError` (4 tests), `stream_parser::is_auth_error` + `infer_mcp_server` (5 tests updated to reflect the new tool-name-based signature), `redirect_server::extract_code` (3 tests).
- Rust-side **binary_resolver** ships with 5 tests (`test_resolve_binaries_returns_struct`, `test_to_path_env_{deduplicates_directories, multiple_directories, handles_none, all_none}`). **Rust-side strict mode** has no dedicated unit test — the `strict_mcp` guard is only tested via integration (spawn would need a real Claude CLI). This is acceptable since the logic is a single `if` gate, but adding a tiny `#[test] fn strict_mcp_blocks_when_no_token()` that introspects just that check would be trivially extractable to a helper function. **Nit, not blocker.**

### 5. Risk Register (WARN — three residual issues)

#### Risk R-3c-1: Stale reauth timer in ChatView (Low severity, MUST FIX)

In `ChatView.tsx:93-97`:
```typescript
setTimeout(async () => {
  unlisten();
  await cancelOAuthFlow();
  setReauthing(false);
}, 5 * 60 * 1000);
```

This timer is NOT cleared inside the success path (the `oauth:token-stored` callback at lines 74-83 calls `unlisten()` and `setReauthing(false)` but does NOT `clearTimeout`). Consequences:
- 5 minutes after a SUCCESSFUL reauth, `cancelOAuthFlow()` is called against an already-completed `pending_server` handle. `tokio::task::JoinHandle::abort()` on a finished task is a no-op — safe, but it clears `pending_verifier` unnecessarily, which is fine unless another reauth is mid-flight (race).
- `setReauthing(false)` is called redundantly (also safe; component may be unmounted by then, causing a React state warning in dev).

**Fix:** Capture the `setTimeout` handle, clearTimeout in the success callback and in the component's unmount effect.

#### Risk R-3c-2: `claude:session-crashed` does not clear mcpStore (Medium severity, MUST FIX)

`useClaudeStream.ts:93-99` handles `session-crashed` by only calling `store().setError(…)`. The `mcpStore.servers` array retains its last-known state — so the UI will show `isConnected: true` for Gmail/Drive/Calendar after the Claude session crashed (and therefore the MCP servers spawned by that Claude died).

**Fix:** Add `useMcpStore.getState().setServers([]);` inside the `session-crashed` listener symmetrically with `session-ended`.

#### Risk R-3c-3: Stale `pending_verifier` on `start_oauth_flow` error (Low severity, NICE-TO-HAVE)

`commands/oauth.rs:18-84` stores the verifier at line 39 BEFORE calling `start_redirect_server` at line 46. If `start_redirect_server` returns Err (e.g., all 10 ports in range occupied), the verifier remains in state. Not exploitable (overwritten on next attempt), but error paths should clean up.

**Fix:** Add a `defer`-style cleanup or early-clear-on-error.

### 6. Spec Alignment (WARN — Golden Rule #12)

- **Phase 3c spec status:** "Complete" ✓ (7ccb355)
- **Plan status:** Not explicitly marked "Complete" in front-matter, but the plan's step-by-step `- [ ]` checkboxes are not updated to `- [x]`. This is a doc hygiene miss but a minor one; the commits are the truth.
- **Parent spec `embedded-claude-architecture.md`:** NOT amended for either strict MCP mode or binary resolver. This violates Golden Rule #12 ("Documentation Stays Current") for user-visible behavior changes.
  - Strict MCP mode introduces an engagement setting that controls whether a session can spawn — that's a material behavior change relative to the parent spec's session-lifecycle description. Parent spec should acknowledge it.
  - Binary resolver crosses the 3c/4a boundary: it was created in 5580ed2 and wired in b89e820, both AFTER the Phase 3c spec was marked Complete (7ccb355). This means those two commits are ambiguously-scoped: they ship on `main` as a single logical batch with 3c, but formally belong to 4a prep. Recommend either (a) explicitly document this in Phase 4a's plan as "prerequisite commits already landed" or (b) move them into Phase 4a's commit range when 4a ships.
- **CLAUDE.md:** No new rule emerged. No update required.
- **Risk register:** Phase 3b carry-forward (tool_id→tool_name) was documented in the 3b final review; Phase 3c addresses it (26dbb71) and closes it. ✓

### 7. Readiness (PASS)

- **Frontend tests (`npx vitest run`)**: 12 test files, 71 tests, 0 failures. Duration 15.89s. Verified live during this review.
- **Rust tests (`cargo test --lib`)**: 58 passed, 0 failed, 0 ignored. Verified live during this review (0.07s runtime post-compile). Includes all 3c additions: `extract_code_*` (3), `is_auth_error_*` (3), `infer_mcp_server_*` (2), `test_generate_config_*` (4), `binary_resolver` (5).
- **Build state:** `cargo check` on main compiles cleanly (confirmed via prior phase review session notes; no new Rust compile errors in any 3c commit per git-log inspection).
- **TypeScript strict:** No `any` introduced; `extractMcpServers` return type is fully inferred from `McpHealth`.

---

## Documentation Enforcement (Golden Rule #12)

- [x] Plan status — commits match the plan 1:1, but plan checkboxes not ticked (minor)
- [ ] Architecture docs — `embedded-claude-architecture.md` NOT updated for strict mode or binary resolver (see Finding 6)
- [x] Spec amended — Phase 3c spec marked Complete (7ccb355)
- [x] Risk register — 3b→3c carry-forward documented, 3c→4a prerequisites implicitly noted in 4a spec/plan
- [x] CLAUDE.md — no new rule emerged
- [ ] Session handoff — no 3c-specific handoff in `.output/` but 3c continuity commits pick up directly into 4a; acceptable

---

## Conditions (Actionable, Priority-Ordered)

1. **MUST FIX IN PHASE 4** — `ChatView.tsx` re-auth `setTimeout` cleanup. File: `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/views/ChatView.tsx:69-104`.
2. **MUST FIX IN PHASE 4** — Clear `mcpStore` on `claude:session-crashed`. File: `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/hooks/useClaudeStream.ts:93-99`.
3. **SHOULD FIX** — Amend `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/embedded-claude-architecture.md` to document strict MCP mode + binary resolver.
4. **NICE-TO-HAVE** — `start_oauth_flow` error-path cleanup. File: `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/commands/oauth.rs:18-84`.
5. **NICE-TO-HAVE** — No UI exposes the `strictMcp` engagement setting yet; it's accessible only by direct IndexedDB edit. Add a toggle in engagement settings UI when Phase 4 ships.

---

## Final Decision

**APPROVED WITH CONDITIONS — 8.5/10**

Phase 3c successfully closes all deferred Phase 3 items and ships the strict-mode + test-coverage quality bar Phase 3 always intended. The architecture is correct, the security posture improved (refresh_token persisted, redirect server localhost-only + one-shot), and the latent 3b `tool_id`-vs-`tool_name` bug was self-caught and fixed (26dbb71) before it could reach users.

Two non-blocking code-correctness issues (`setTimeout` leak in re-auth; missing `mcpStore` clear on `session-crashed`) must be fixed in Phase 4. One documentation gap (parent spec not amended for strict mode + binary resolver) violates Golden Rule #12 and must be closed within the same phase.

The binary resolver belongs conceptually to Phase 4a but landed on `main` as part of the 3c tail — this cross-phase boundary should be made explicit in the Phase 4a plan to avoid audit ambiguity when 4a ships.

Score deductions:
- −1.0 for the two MUST-FIX items (reauth timer, session-crashed mcpStore)
- −0.5 for parent-spec doc staleness (Golden Rule #12 partial)

---

## Finalization Stamp (2026-04-16, post-test re-verification)

Empirical test results re-run at HEAD `b4bb80a` prior to sealing this review:

| Suite | Command | Result |
|-------|---------|--------|
| Rust `src-tauri` | `cargo test --lib` | **58 passed, 0 failed, 0 ignored** — 0.04s |
| Frontend | `npm test -- --run` (Vitest 4.1.4) | **71 passed across 12 test files** — 15.38s |

No flakes, no skipped assertions. All Phase 3c additions (`test_generate_config_*`, `binary_resolver::tests::*`, `stream_parser::tests::test_is_auth_error_*`, `test_infer_mcp_server_*`, `oauth::redirect_server::tests::test_extract_code_*`, `oauth::token_refresh::tests::*`, and the Vitest files covering `mcp-utils`, `mcpStore`, `claudeStore-auth`) green on the current `main`.

**Status: FINALIZED.** Decision and score unchanged: **APPROVED WITH CONDITIONS — 8.5/10.** Conditions 1–3 roll forward into Phase 5 / next M2 polish wave, tracked in the 2026-04-16 handoff.
