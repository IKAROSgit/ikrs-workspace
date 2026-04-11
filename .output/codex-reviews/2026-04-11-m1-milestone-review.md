# CODEX REVIEW

**Subject:** IKAROS Workspace M1 — Full Milestone Review
**Type:** phase-review (Tier 3 — all 8 phases, 21 tasks)
**Date:** 2026-04-11
**Reviewed by:** Codex (superpowers:code-reviewer agent)
**Commit range:** ff2f1b5..46a605e (12 commits)
**Repository:** IKAROSgit/ikrs-workspace

---

## VERDICTS

| # | Criterion | Verdict | Justification |
|---|-----------|---------|---------------|
| 1 | Structural | **PASS** | Modules properly organized with clean Rust->IPC->TypeScript layer separation. Commands, MCP, OAuth cleanly separated in Rust; stores, hooks, providers, views cleanly separated in React. File structure closely matches the plan. |
| 2 | Architecture | **WARN** | Implementation matches spec architecture. Zustand stores correctly avoid getter properties (CODEX MUST-FIX #2 resolved in engagementStore, taskStore, uiStore). Providers own Firestore sync, stores own client-side state, hooks compose both. **Violation found:** mcpStore.ts had a `getServer` getter using `get()` — the exact pattern MUST-FIX #2 prohibits. |
| 3 | Security | **WARN** | CSP configured in tauri.conf.json. PKCE OAuth correctly avoids client_secret. Credentials stored in OS keychain. **Issues:** (a) CODEX MUST-FIX #1 (age encryption for vault archives) NOT implemented — archives are unencrypted tar.gz. `age` and `secrecy` crates absent from Cargo.toml. (b) Tauri capabilities use `:default` for all plugins (broader than necessary). (c) `health_check` used `.unwrap()` on mutex lock. |
| 4 | Completeness | **WARN** | All 21 tasks structurally present. 4 hooks (useGmail, useCalendar, useDrive, useNotes) are placeholder implementations returning empty arrays — documented in plan Gap 1 as expected. **Critical:** Tasks 10-21 were NOT committed — 860 lines in working tree only. |
| 5 | Risk register | **PASS** | Plan identifies 5 gaps with clear rationale for deferral. R9 (MCP supply chain) tracked. Gaps 1-5 all have recommendations and execution order notes. |
| 6 | Spec alignment | **WARN** | Two notable deviations: (a) Plan specifies `main.rs` as entry point but implementation uses `lib.rs` (justified Tauri convention). (b) Plan specifies `age` encryption (MUST-FIX #1) but not implemented. (c) Tilde path `~/.ikrs-workspace/vaults/` in useEngagement.ts won't expand in process spawn — tilde is a shell feature, not available in programmatic spawn. |
| 7 | Readiness | **PASS** | Codebase well-structured for M2. MCP bridge can be added via new Rust commands + wiring into existing hooks without refactoring. Hook interfaces already have the right shape for consuming MCP data. |

---

## DECISION: APPROVED WITH CONDITIONS

**Score: 7/10**

---

## CONDITIONS (must fix before M1 is considered shipped)

### C1 (Critical): Commit the work

Tasks 10-21 existed only as uncommitted working tree changes. 860 lines of code including all Rust command modules, all 7 view implementations, error boundaries, CI workflow, and hooks had zero commit protection. A single `git checkout .` would have destroyed all work.

**Status: FIXED** — Committed as `46a605e` (34 files, 1864 insertions).

### C2 (Important): Remove mcpStore getter violation

`src/stores/mcpStore.ts` line 8-9 and 20 defined `getServer` using `get()`. This is the exact CODEX MUST-FIX #2 pattern — Zustand getters are not reactive. Components should use selectors instead: `useMcpStore((s) => s.servers.find(srv => srv.type === type))`.

**Status: FIXED** — `getServer` removed from interface and implementation. No consumers existed.

### C3 (Important): Document age encryption deferral

CODEX MUST-FIX #1 requires `age` encryption for vault archives. The plan states: "client engagement data must not sit on disk unencrypted" (spec section 6.3). The `archive_vault` function creates plain `.tar.gz` files. This was silently skipped — must be documented as tracked debt.

**Status: FIXED** — Added explicit deferral note in plan file with M2 Phase 1 timeline.

### C4 (Suggestion): Fix health_check unwrap

`src-tauri/src/mcp/manager.rs` line 83 used `self.processes.lock().unwrap()` which panics on poisoned mutex. All other methods in the same struct correctly use `.map_err(|e| e.to_string())?`.

**Status: FIXED** — Changed return type to `Result<McpStatus, String>`, updated calling command in `commands/mcp.rs`.

---

## ADDITIONAL FINDINGS

### F5 (Important): Tilde path won't expand

`src/hooks/useEngagement.ts` line 76 used literal `~/.ikrs-workspace/vaults/`. Tilde expansion is a shell feature; it won't resolve in a programmatic process spawn context.

**Status: FIXED** — Replaced with `homeDir()` from `@tauri-apps/api/path`.

### F6 (Important): Firestore rules not updated

The app writes to `consultants`, `clients`, `engagements`, `credentials`, and `tasks` collections, but `firestore.rules` in the monorepo has no rules for these. Under deny-all base policy, all writes will fail in production.

**Status: OPEN** — Requires separate update to `ikaros-platform/firestore.rules`.

### F7 (Important): next-themes in dependencies

`package.json` included `next-themes` (a Next.js-specific package). This is a Tauri+Vite app — not appropriate. Used by `sonner.tsx` for theme detection.

**Status: FIXED** — Removed `next-themes`, rewired `sonner.tsx` to use `uiStore.theme`.

### F8 (Suggestion): Missing taskStore test

`engagementStore` has tests; `taskStore` does not. Plan file structure lists `tests/unit/stores/taskStore.test.ts` but it doesn't exist.

**Status: OPEN** — Low priority, can be added in M2.

### F9 (Suggestion): Broad Tauri permissions

`src-tauri/capabilities/default.json` uses `:default` scope for all 9 plugins. For production, should be narrowed (e.g., `fs:allow-read` for specific paths, `shell:allow-spawn` with command allowlist).

**Status: OPEN** — Acceptable for M1 development, should tighten for production release.

---

## POSITIVE OBSERVATIONS

1. **Task-parser TDD** — 10 meaningful tests covering parse, render, and round-trip. Bidirectional markdown parser is solid domain logic.
2. **PKCE implementation** — Clean Rust: 32-byte random verifier, SHA-256 challenge, base64url without padding, 2 unit tests.
3. **Zustand store pattern** — `engagementStore.ts` is textbook correct: action functions via `set`, no getters, clear comment trail.
4. **Engagement switching** — `useEngagement.ts` correctly sequences: kill MCP -> load credentials -> set state -> ensure vault -> spawn new MCP. Per-server error handling.
5. **ViewErrorBoundary** — `key={activeView}` ensures error state resets on view switch.
6. **CI pipeline** — Well-structured with job dependencies and cross-platform build matrix.
7. **Consistent view patterns** — All 7 views handle "no engagement selected" and "not connected" states gracefully.
8. **Type alignment** — Types and Zod schemas perfectly aligned, no drift.

---

## RECOMMENDATIONS FOR M2

### Priority 1: MCP Client Bridge (Gap 1)

This is the single most impactful piece missing. Without it, 4 of 7 views show empty states. Implementation:
- Add Rust-side MCP client that holds stdin/stdout per process
- Expose `mcp_request(server_type, method, params) -> Result` Tauri command
- Wire into existing useGmail/useCalendar/useDrive/useNotes hooks
- No refactoring needed — hook interfaces already have the right shape

### Priority 2: Age Encryption (MUST-FIX #1)

Security requirement from spec 6.3. Add `age` + `secrecy` crates, pipe archive through `age::Encryptor`, store key in OS keychain.

### Priority 3: Firestore Rules (F6)

The app literally cannot write data in production without this. Update `firestore.rules` with per-collection rules scoped to authenticated user.

### Priority 4: Google OAuth Client

The "Connect Google Account" flow needs a real OAuth client ID configured as `VITE_GOOGLE_OAUTH_CLIENT_ID`.

---

## DOCUMENTATION CHECKLIST

- [x] Plan status updated (all 21 tasks complete)
- [x] Architecture deviation documented (lib.rs vs main.rs)
- [x] Age encryption deferral documented in plan
- [ ] Session handoff document (pending)
- [ ] Firestore rules update (pending, in monorepo)
- [x] CLAUDE.md — no new rules emerged
- [x] Risk register — no new risks beyond tracked gaps

---

## VERIFICATION DATA

| Check | Result |
|-------|--------|
| `cargo check` | PASS (2 pre-existing warnings: dead code in credentials.rs and manager.rs) |
| `npm run build` (tsc + vite) | PASS (0 errors, chunk size advisory only) |
| JS tests (vitest) | 19/19 PASS across 3 test files |
| Rust tests (cargo test --lib) | 2/2 PASS (PKCE generation + uniqueness) |
| Commit status | All work committed as 46a605e |
| Push status | Pushed to IKAROSgit/ikrs-workspace main |
