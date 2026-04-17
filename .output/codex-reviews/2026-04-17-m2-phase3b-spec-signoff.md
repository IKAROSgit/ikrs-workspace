# Codex Sign-Off: M2 Phase 3b — MCP Wiring + Token Resilience

**Date:** 2026-04-17
**Reviewer:** Codex (7-point validation)
**Target:** `docs/specs/m2-phase3b-mcp-wiring-design.md` + companion plan `docs/superpowers/plans/2026-04-12-m2-phase3b-mcp-wiring.md` + shipped code (commits 64653ae..d1442ef)
**Verdict:** PASS WITH CONDITIONS (overall)
**Score:** 8/10

## Summary
Retroactive sign-off for a spec + plan + implementation trio that shipped Phase 3b cleanly. The spec is internally consistent, matches the parent architecture amendment (Q3 "spawn time"), and the shipped code is a faithful superset of what was promised. Two governance gaps warrant WARNs: the spec/plan were never added to git (Golden Rule #1 — one repo, one truth) and the `src-tauri/src/mcp/` directory was emptied but not deleted, leaving an orphan folder that contradicts the "full retirement" claim in Section 4.

## 7-Point Results

### 1. Structural validation — PASS
- Dependency DAG is clean: `commands.rs` → `mcp_config.rs` + `oauth::token_refresh` + `session_manager.rs`. No circular deps. (`src-tauri/src/claude/commands.rs:1-86`)
- 10 tasks across 4 waves with explicit parallelism metadata. Wave 2 correctly identifies `4 ∥ 5, then 6` dependency.
- All 4 subsystems (config gen, spawn injection, retirement, token resilience) are covered.
- Cross-cutting: atomic writes (`.mcp-config.json.tmp` → rename, `mcp_config.rs:93-99`), session registry untouched, binary resolver plumbed through (`commands.rs:49-57`).

### 2. Architectural consistency — PASS
- Shipped code matches spec Section 1 almost exactly. `generate_mcp_config` signature added an `npx_path` param not in the original spec (`mcp_config.rs:23-28`) — this is a Phase 4a sandbox fix that post-dates 3b; documented in commit `b89e820`.
- `session_manager.rs:79-82` wires `--mcp-config` exactly as specified. `.envs(&env_vars)` at `session_manager.rs:110` matches Section 2.
- Parent spec Q3 amendment landed in commit `d1442ef`: `embedded-claude-architecture.md:995` now reads "generated at session spawn time (not scaffold time — token availability changes between scaffold and spawn)". Codex I1 resolved.
- Layer boundaries (`CLAUDE.md` Golden Rule #7) respected: Rust orchestrator owns keychain + filesystem + spawn; frontend only passes slug.

### 3. Security audit — PASS
- `${GOOGLE_ACCESS_TOKEN}` placeholder in `mcp_config.rs:42-44,53-55,64-66` — actual token never written to disk. Verified by reading the generated JSON in test output: env value is literally the `${...}` string.
- Token flows through process env via `.envs(&env_vars)` (`session_manager.rs:110`) — inherited by Claude CLI, which passes it to MCP server children. No file-based leakage.
- Keychain key format `ikrs:{engagement_id}:google` via `make_keychain_key` helper (`commands.rs:19`). Service name centralized in `credentials.rs`.
- `.mcp-config.json` added to `.gitignore:23-24` (commit `66dfb10`) — prevents accidental commit of per-engagement config. Good hygiene.
- `claude::auth` confirmed separate from MCP path; no cross-contamination.
- Risk table (spec line 236) correctly identifies "no secrets in file — token passed via env var only" as the isolation property.

### 4. Completeness — WARN
- Success criteria (spec lines 242-249) are concrete and testable. All 7 appear exercised by the shipped code.
- Four unit tests for `generate_mcp_config` (`mcp_config.rs:104-177`) cover the matrix (token × vault presence).
- Five unit tests for `is_auth_error` / `infer_mcp_server` (`stream_parser.rs:413-434`).
- **Gap 1:** The spec's Section 5 flow (step 6: "kills current Claude session, respawns with fresh token") was implemented differently. Actual shipped `ChatView.tsx:74-80` listens for `oauth:token-stored` event, not direct `exchangeOAuthCode` → `storeCredential`. This is an *improvement* (event-driven vs polling) but the spec was not amended to reflect the two-step OAuth redirect-server design that landed in commits `7715fe8`, `05670d0`, `9cbf647`. Golden Rule #12 violation — docs did not stay current with the delta.
- **Gap 2:** "Obsidian entry is included only if the vault directory exists on disk" (spec line 85) — verified at `mcp_config.rs:72`. But `commands.rs:42-46` creates the directory unconditionally for any engagement with a client slug, so Obsidian is effectively always present when a client slug is supplied. The two rules are consistent in practice but the spec wording implies a gating condition that is actually always satisfied.

### 5. Risk register — WARN
All six risks in the spec (lines 232-239) remain relevant:
- `npx cold start latency` — still present; Phase 4a binary-resolver reduces but does not eliminate it.
- `Token expiry mid-session` — mitigation shipped (auth-error detection + re-auth toast). Verified at `stream_parser.rs:281-300` and `ChatView.tsx:69-92`.
- `macOS App Sandbox blocks npx` — mitigated by Phase 4a binary resolver (`commands.rs:49-57`). This risk was promoted out of "future" into "addressed" during 3b→4a but the 3b spec's risk table was not updated.
- `Consultant's personal MCP servers conflict` — mitigation is `--mcp-config` additive mode (spec line 86). Correctly implemented (no `--strict-mcp-config` in Phase 3b args).
- `Resume after mid-turn kill for re-auth` — "accepted" risk per spec; still accepted.
- **New risk introduced, not logged:** The spec omits a risk around `infer_mcp_server` falling back to `"unknown"` when `tool_name_map` is missing an entry. The shipped fix (Codex C2, commit `26dbb71`) uses the tool_name_map correctly, but the spec's original algorithm (infer from `tool_id` substring) was buggy — any tool with "drive" in its ID would misattribute. Fixed in code but the spec still shows the buggy pseudocode at the Phase 3b design level (spec line ~172 describes detection without mentioning the map).

### 6. Spec/plan alignment — PASS
- Plan's 10 tasks map 1:1 to spec sections. Task 1-3 = Section 1-3; Task 4-5 = Section 4; Task 6 = Section 3 (clientSlug plumbing); Task 7-10 = Section 5; doc task = Section's Codex I1 resolution.
- Plan introduces `client_slug` as `Option<String>` (plan line 174, "Codex I3 fix") which extends the spec's `String` (spec line 120) — a beneficial deviation, correctly reflected in shipped `commands.rs:10`.
- Plan explicitly calls out mcpStore consumer audit (plan line 315) — a governance addition the spec didn't require.
- No scope creep. `strict_mcp` parameter in shipped `commands.rs:11` is Phase 3c scope (commit `02c708b`), additive and backwards-compatible.

### 7. Implementation readiness (fidelity check) — WARN
- `mcp_config.rs`, `session_manager.rs`, `commands.rs` match the plan pseudocode nearly verbatim.
- Auth-error detection: shipped version at `stream_parser.rs:281-300` uses `tool_name_map` to resolve the tool name, not the raw `tool_id` as the plan's snippet (plan line 422) proposed. This is a **beneficial correction** (Codex C2, commit `26dbb71`) but the plan was not amended.
- **Issue 1 (Governance):** Both the spec (`docs/specs/m2-phase3b-mcp-wiring-design.md`) and plan (`docs/superpowers/plans/2026-04-12-m2-phase3b-mcp-wiring.md`) are **untracked in git** (verified: `git status` shows both as "Untracked files"). This violates CLAUDE.md Golden Rule #1 ("ONE REPO, ONE TRUTH") and Golden Rule #12 ("DOCUMENTATION STAYS CURRENT"). Architecture docs that governed 23 commits to main must be version-controlled.
- **Issue 2 (Orphan folder):** `src-tauri/src/mcp/` exists as an empty directory (`ls -la` confirms only `.` and `..`). Spec Section 4 promised "Files to delete: `src-tauri/src/mcp/manager.rs`, `src-tauri/src/mcp/mod.rs`". The files are deleted but the directory shell remains. This contradicts Golden Rule #2 ("CLEAN, NOT PENDING"). Low severity but a literal spec-vs-code mismatch.
- **Issue 3 (Test gap):** No integration test verifying `spawn_claude_session` end-to-end with a real `.mcp-config.json` path passed to `--mcp-config`. Unit tests cover config generation and auth-error parsing in isolation. Acceptable for 3b; Phase 3c's test additions (`29b47a4`, `44e3690`, `ead4fb3`) narrow the gap.

## Conditions (WARN — must address before Phase 4 closes or explicitly waive)

1. **[Important]** Commit the Phase 3b spec and plan to git. Both files have been governing main for days while untracked. Command: `git add docs/specs/m2-phase3b-mcp-wiring-design.md docs/superpowers/plans/2026-04-12-m2-phase3b-mcp-wiring.md` and commit with a backdated note explaining retroactive tracking.
2. **[Important]** Delete the empty `src-tauri/src/mcp/` directory. This honors the spec's "full retirement" claim and Golden Rule #2.
3. **[Suggestion]** Amend the spec's risk table (lines 232-239) with a closing column or postscript noting which risks were retired in Phase 4a (sandbox/npx) and which were corrected in Phase 3c (infer_mcp_server via tool_name_map).
4. **[Suggestion]** Amend spec Section 5 re-auth flow (lines 182-189) to reflect the event-driven `oauth:token-stored` design actually shipped, so future readers don't reimplement a different flow from the spec.

## Blockers
None. The implementation is architecturally sound, the security posture is correct, and the code is production-ready.

## Strengths
- **Atomic file write** (`mcp_config.rs:93-99`) — tmp + rename prevents torn reads on crash.
- **Env-var placeholder pattern** — token literally never touches disk; the JSON on disk is identical across users with valid tokens. Clean isolation boundary.
- **Option A decision rationale** (spec lines 18-24) is articulated with explicit rejections of B, C1, C3. Codex-approved trail is auditable.
- **`client_slug: Option<String>`** correctly handles the edge case of engagements without linked clients (plan line 174, shipped at `commands.rs:36-59`).
- **Additive `--mcp-config` mode** preserves consultant's personal MCP servers — respects the "consultant as power user" principle.
- **Four-matrix test coverage** for config generation (`mcp_config.rs:110-177`) is concise and exhaustive.
- **Codex review cycle** is visible in the commit history: C1 (vault orphan), C2 (tool_name_map), I1 (spawn-time), I2 (strict resume skip) were all addressed in distinct commits.
- **McpProcessManager retirement** is complete at the code level: no `mod mcp;`, no `McpProcessManager` references, no `commands::mcp::*` registrations — verified by grep returning zero matches.

## Sign-off Decision
**PROCEED WITH CONDITIONS.**

The Phase 3b work as a whole — spec, plan, and shipped code — is architecturally sound and deserves sign-off. The two governance WARNs (untracked docs, orphan directory) are mechanical cleanup, not design flaws. Fix them in the next commit to close the loop.

---

**Reviewed files (absolute paths):**
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m2-phase3b-mcp-wiring-design.md`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/superpowers/plans/2026-04-12-m2-phase3b-mcp-wiring.md`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/embedded-claude-architecture.md` (Q3 line 995)
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/mcp_config.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/commands.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/session_manager.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/stream_parser.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/lib.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/views/ChatView.tsx`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/hooks/useWorkspaceSession.ts`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/.gitignore` (lines 23-24)
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/mcp/` (empty orphan directory)
