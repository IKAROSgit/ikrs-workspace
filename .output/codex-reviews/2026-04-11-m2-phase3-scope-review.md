# Codex Architectural Review -- M2 Phase 3 Scope Proposal

> **Reviewer:** Codex (Claude Opus 4.6)
> **Date:** 2026-04-11
> **Scope:** Phase 3 scope proposal (Option C: Full Spec) for IKRS Workspace
> **Prior phases:** Phase 1 (PASS 8/10), Phase 2 (PASS 10/10)
> **Build status at review time:** cargo check PASS (13 warnings, 0 errors), tsc PASS, vitest 34/34 PASS

---

## VERDICT: WARN -- Scope is sound but needs decomposition and 3 architectural corrections

**Score: 7/10** -- the right features are proposed, but the scope is too large for one phase, and three design decisions need correction before execution begins.

---

## 1. ANSWERS TO THE SEVEN QUESTIONS

### Q1: Should this be one Phase 3 or split?

**SPLIT. Strongly recommended: Phase 3a and Phase 3b.**

Phase 3 as proposed contains 10 work items spanning 4 groups across 2 layers (Rust backend + React frontend), touching 3 different subsystems (session management, MCP infrastructure, UI). This is too broad for a single coherent phase with a clean review checkpoint.

**Phase 3a: Session Management + UX Polish** (items 1-6)
- ToolActivityCard collapsible details
- SessionIndicator detail modal
- Chat history per engagement (in-memory)
- Session resume (--resume flag)
- Engagement switching (full sequence)
- Orphan PID cleanup

**Phase 3b: MCP Wiring + Resilience** (items 7-10)
- Per-engagement MCP config generation
- Wire Google MCP servers via --mcp-config
- Offline detection on spawn
- Mid-session network loss handling

**Rationale:** Phase 3a is purely internal state management and UI -- it can be tested without network or Google credentials. Phase 3b requires MCP server packages, Google OAuth tokens, and network manipulation for testing. Different test prerequisites = different phases.

### Q2: Is SQLite justified for 2 tables?

**YES -- and it is already a dependency.** `tauri-plugin-sql` with SQLite feature is in `Cargo.toml` (line 20) and initialized in `lib.rs` (line 16). The plugin is loaded. You are paying the binary size cost already. Adding 2 tables (sessions, orphan_pids) costs zero additional dependencies.

However, note that `tauri-plugin-sql` runs SQL from the frontend (JavaScript side), not from Rust commands. For orphan cleanup, which runs in Rust at startup (`lib.rs`), you need either:
- **(A)** Use `tauri-plugin-sql` from the JS side and trigger cleanup after webview loads (adds startup delay, race condition risk)
- **(B)** Add `rusqlite` as a direct Rust dependency for backend-only DB access (cleaner, no webview dependency)
- **(C)** Use a simple JSON file at `{app_data_dir}/session-registry.json` instead of SQLite (simplest, sufficient for <100 records)

**Recommendation: Option C.** A JSON file with `Vec<SessionRecord>` serialized via serde is sufficient for tracking PIDs and session IDs. SQLite is overkill for this data volume. Reserve the existing `tauri-plugin-sql` for future features that genuinely need relational queries (e.g., engagement metadata search).

### Q3: MCP architecture -- CLI-managed or app-managed?

**CLI-managed via `--mcp-config` is correct. The existing app-side McpProcessManager should be retired for Google servers.**

This is the single most important architectural decision in Phase 3. Here is the full analysis:

**Current state:** `useEngagement.ts` spawns Gmail/Calendar/Drive/Obsidian MCP processes independently via `McpProcessManager`. These processes run alongside Claude CLI but are NOT connected to it. Claude CLI has no knowledge of them. This means Claude cannot use Gmail, Calendar, or Drive tools -- the MCP servers are running but unreachable from Claude's perspective.

**The fix:** Claude CLI's `--mcp-config` flag tells it to spawn and manage MCP servers itself. When Claude starts with `--mcp-config engagement/.mcp-config.json`, it discovers the Gmail, Calendar, Drive, and Obsidian tools and can use them directly.

**Concrete change:**
1. At scaffold time, generate `{engagement_path}/.mcp-config.json` containing server definitions
2. On `spawn_claude_session()`, pass `--mcp-config {engagement_path}/.mcp-config.json` to the CLI
3. Use `--strict-mcp-config` to prevent the consultant's personal `.mcp.json` from leaking servers across engagements
4. Remove the app-side MCP spawning from `useEngagement.ts` for the 4 Google/Obsidian servers
5. Keep `McpProcessManager` alive ONLY if there are future non-Claude MCP needs (otherwise delete it)

**The `.mcp-config.json` format:**
```json
{
  "mcpServers": {
    "gmail": {
      "command": "npx",
      "args": ["@shinzolabs/gmail-mcp@1.7.4"],
      "env": { "GOOGLE_ACCESS_TOKEN": "${CREDENTIAL}" }
    },
    "calendar": {
      "command": "npx",
      "args": ["@cocal/google-calendar-mcp@2.6.1"],
      "env": { "GOOGLE_ACCESS_TOKEN": "${CREDENTIAL}" }
    },
    "drive": {
      "command": "npx",
      "args": ["@piotr-agier/google-drive-mcp@2.0.2"],
      "env": { "GOOGLE_ACCESS_TOKEN": "${CREDENTIAL}" }
    },
    "obsidian": {
      "command": "npx",
      "args": ["@bitbonsai/mcpvault@1.3.0", "{vault_path}"]
    }
  }
}
```

**Critical issue with credentials:** The `GOOGLE_ACCESS_TOKEN` is stored in the OS keychain (via `tauri-plugin-keyring`). Claude CLI cannot read the keychain. The app must:
1. Read the token from keychain at spawn time
2. Write a resolved `.mcp-config.json` (with actual token, not placeholder) to a temp location
3. Pass the temp file path to `--mcp-config`
4. Delete the temp file after the session ends

Alternatively, pass the token as an environment variable to the Claude CLI process, and reference it in the config as `${GOOGLE_ACCESS_TOKEN}`. The Claude CLI resolves env vars in MCP config. This is cleaner -- no temp file, no token on disk.

**Recommendation:** Pass `GOOGLE_ACCESS_TOKEN` as an env var to the Claude CLI subprocess, reference `${GOOGLE_ACCESS_TOKEN}` in the static `.mcp-config.json`. Modify `spawn()` in `session_manager.rs` to accept and inject environment variables.

### Q4: Chat history persistence

**In-memory Map is correct for now. Do not use SQLite. Do not rely solely on `~/.claude/`.**

The proposal's `Map<engagementId, ChatMessage[]>` is the right design. Here is why each option fails or succeeds:

- **SQLite for messages:** Overkill. Chat messages are ephemeral session artifacts, not permanent records. The consultant's Claude subscription handles session persistence in `~/.claude/`. Adding app-side message persistence creates a data duplication problem and a privacy surface (two copies of potentially sensitive conversation data).

- **Relying solely on `~/.claude/`:** Insufficient. When the consultant switches engagements and returns, they need to see the chat history from their current app session. Claude CLI's `--resume` will resume the session state, but the app's UI state (the rendered messages in `claudeStore.messages`) is gone. The in-memory Map bridges this gap for the current app session.

- **In-memory Map:** Correct tradeoff. Messages survive engagement switching within the same app session. Messages are lost on app restart. On restart, the consultant clicks "Connect" and gets a fresh session (or resumes via `--resume`, which replays from Claude's perspective but with a clean UI). This is the behavior users expect from chat applications.

**One refinement:** Cap the Map at a reasonable size. 50 messages per engagement, FIFO eviction. Prevents memory bloat for long sessions.

### Q5: Risk assessment for Phase 3

| ID | Risk | Severity | Mitigation |
|----|------|----------|------------|
| P3-R1 | `--resume` with stale session_id causes CLI error or hang | MEDIUM | Wrap resume attempt in timeout (5s). If CLI does not emit `system.init` within 5s, kill and fall back to fresh session. |
| P3-R2 | `--strict-mcp-config` blocks consultant's personal MCP servers they rely on | MEDIUM | Make `--strict-mcp-config` opt-in in app settings. Default to `--mcp-config` (additive) for the first release. |
| P3-R3 | Google OAuth token expired when session spawns | LOW | Token refresh is handled by the app's OAuth flow (already implemented in `oauth.rs`). If token is expired at spawn time, the MCP servers will fail gracefully -- Claude will report tool errors, not crash. |
| P3-R4 | Orphan cleanup sends SIGTERM to wrong PID (PID reuse) | LOW | Check process name/command before killing. On macOS: `ps -p {pid} -o comm=` should contain "claude". On Linux: read `/proc/{pid}/cmdline`. |
| P3-R5 | Engagement switch race condition -- user clicks fast | MEDIUM | Debounce engagement switching. Disable the switcher UI during switch (already done via `switching` state). Add a queue or ignore-if-switching guard in `switchEngagement()`. |
| P3-R6 | `--mcp-config` flag format changes between Claude CLI versions | MEDIUM | Same mitigation as R1/R6 in the original risk register -- pin minimum CLI version, test with current version (2.1.92). |

### Q6: Feasibility -- can this be one session?

**No.** Even split into 3a/3b, each sub-phase is 10-12 tasks. A single session can reliably execute ~15-20 tasks with review. Phase 3a fits in one session. Phase 3b fits in one session.

**Estimated task counts:**
- Phase 3a: ~14 tasks (6 features x ~2.3 tasks each: impl + type changes + tests)
- Phase 3b: ~10 tasks (4 features, but MCP wiring is complex: scaffold changes, session_manager changes, useEngagement refactor, config generation, env injection, tests)

### Q7: Conflicts with Phase 4 (distribution)?

**Two potential conflicts:**

1. **macOS App Sandbox + MCP subprocess spawning:** When the app is sandboxed (Phase 4), `npx` invocations from Claude CLI's MCP config may fail because the sandbox restricts subprocess execution paths. This is not a Phase 3 problem -- but Phase 3's design should use absolute paths to MCP server binaries (not `npx`) to make Phase 4 easier. **Action:** In `.mcp-config.json`, resolve `npx @package@version` to the actual binary path at scaffold time. Store the resolved path.

2. **Security-Scoped Bookmarks + `--mcp-config` file location:** If `.mcp-config.json` lives inside the workspace folder (which it should), the Security-Scoped Bookmark that grants access to the workspace folder also covers this file. No conflict.

3. **Code signing + `--disallowed-tools`:** No conflict. The CLI flags are passed as arguments, not modifying signed binaries.

**Verdict:** One minor Phase 4 prep item (resolve npx to absolute paths). Otherwise clean.

---

## 2. ARCHITECTURAL CORRECTIONS (must address before execution)

### C1: CRITICAL -- `monitor_process` does not clean up the session map

**File:** `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/session_manager.rs`
**Lines:** 179-218

The `monitor_process` function detects when the Claude CLI exits and emits events to the frontend. But it does NOT remove the dead session from `self.sessions`. This means:

- After a crash, `has_session()` returns `true` (line 173-175)
- Attempting to spawn a new session hits the `max_sessions` limit and tries to kill the dead session by dropping its stdin -- which is already closed
- The dead session entry leaks in the HashMap forever (until app restart)

**Fix:** `monitor_process` needs a reference to the sessions map (or a cleanup callback) to remove the dead session. Pass `Arc<Mutex<HashMap<String, ClaudeSession>>>` to the monitor task, or use a `tokio::sync::mpsc` channel to signal the manager.

This is a pre-existing bug from Phase 1 that Phase 3's engagement switching will make worse (frequent session creation/destruction amplifies the leak).

### C2: IMPORTANT -- `useEngagement.ts` and Claude session are not coordinated

**File:** `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/hooks/useEngagement.ts`

Currently, `switchEngagement()` kills MCP servers and spawns new ones, but does NOT interact with the Claude session at all. `ChatView.tsx` manages the Claude session independently. Phase 3's engagement switching item (item 5) must unify these:

1. `switchEngagement()` must: kill Claude session -> save chat history -> kill MCPs -> set active engagement -> spawn new Claude session (with `--mcp-config`) -> load chat history
2. The current split between `useEngagement.ts` (MCP lifecycle) and `ChatView.tsx` (Claude lifecycle) will create race conditions when both try to manage the switch independently.

**Recommendation:** Create a single `useWorkspaceSession()` hook that orchestrates the full lifecycle: Claude session + MCP servers + chat history. This replaces the current ad-hoc coordination.

### C3: IMPORTANT -- Stream parser drops `tool_input` data needed for collapsible details

**File:** `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/stream_parser.rs`
**Lines:** 192-203

The stream parser reads `block["input"]` and passes it to `friendly_label()` to generate a short string, then discards the raw input. For Phase 3 item 1 (collapsible ToolActivityCard), the raw input needs to be forwarded to the frontend.

**Fix:** Add `tool_input: Option<serde_json::Value>` to `ToolStartPayload`. Serialize the full input object (with a size cap -- truncate inputs >4KB to prevent memory issues with large file contents). Update the TypeScript `ToolStartPayload` type to include `tool_input?: Record<string, unknown>`.

---

## 3. DESIGN FEEDBACK ON INDIVIDUAL ITEMS

### Item 1: ToolActivityCard collapsible details -- APPROVED

Good scope. The Rust-side change (adding `tool_input` to payload) is small. The React-side change is straightforward: add a `useState(false)` for expanded state, render input params in a `<pre>` block when expanded. The `ToolEndPayload.summary` already exists for showing results.

One addition: also show `tool_result` content in the expanded view. Currently `summarize_tool_result()` truncates to 80 chars. For the expanded view, forward the full (but capped) result. Add `tool_result_full: Option<String>` to `ToolEndPayload` (cap at 2KB).

### Item 2: SessionDetailsModal -- APPROVED with scope reduction

The proposal includes engagement name, session duration, token cost breakdown, and session_id. This is fine, but "token cost breakdown" is problematic -- Claude CLI's `result` event only gives `total_cost_usd`, not a per-model or per-turn breakdown. The stream-json format does not expose input/output token counts.

**Scope reduction:** Show total session cost, session duration, model name, engagement name, session_id. No per-turn breakdown (data not available).

### Item 3: Chat history per engagement -- APPROVED

In-memory `Map<string, ChatMessage[]>` is the right design. See Q4 above. Add the 50-message cap.

### Item 4: Session resume -- APPROVED with timeout guard

Pass `--resume {session_id}` when the engagement has a stored session_id. The `--resume` flag is confirmed available in Claude CLI 2.1.92. Add a 5-second timeout on the init event -- if no `system.init` arrives, kill and spawn fresh.

Store the mapping `engagement_id -> session_id` in the engagement store (Zustand) or the session registry JSON file. Zustand is simpler for this.

### Item 5: Engagement switching -- APPROVED, needs unified hook (see C2)

The proposal correctly identifies the sequence: kill -> swap -> spawn. The `EngagementSwitcher` already shows a loading state. The missing piece is C2 above -- unifying the Claude session and MCP lifecycle.

### Item 6: Orphan PID cleanup -- APPROVED with JSON registry (see Q2)

Use a JSON file instead of SQLite. Write PIDs on spawn, read on startup, check liveness, clean up. Simple and sufficient.

### Item 7: Per-engagement MCP config -- APPROVED

Generate `.mcp-config.json` at scaffold time. See Q3 above for the format and credential injection strategy.

### Item 8: Wire Google MCP servers -- APPROVED, this is the highest-value item

See Q3 above for the full architectural decision. Key points:
- Pass `GOOGLE_ACCESS_TOKEN` as env var to Claude CLI subprocess
- Use `--mcp-config` (not `--strict-mcp-config` by default)
- Remove app-side MCP spawning for Google servers
- Keep `McpProcessManager` for now but mark it for potential removal

### Item 9: Offline detection -- APPROVED

Simple network check before spawn. Emit `claude:error`. Inline in InputBar. No modal. This is 1-2 tasks.

### Item 10: Mid-session network loss -- APPROVED, already partially implemented

The stream parser already handles `result` events with `is_error: true` (lines 248-259 of stream_parser.rs). The gap is the retry button in InputBar, which is a small UI change.

---

## 4. RECOMMENDED PHASE 3a TASK LIST

1. Fix C1: session map cleanup in `monitor_process` (bug fix, prerequisite)
2. Add `tool_input` to `ToolStartPayload` (Rust types + stream parser)
3. Add `tool_input` to TypeScript `ToolStartPayload` type
4. Implement collapsible `ToolActivityCard` with expand/collapse state
5. Implement `SessionDetailsModal` component
6. Wire SessionIndicator click to open SessionDetailsModal
7. Add `chatHistoryMap` to claudeStore (Map<engagementId, ChatMessage[]>)
8. Implement save/load/clear cycle for chat history on engagement switch
9. Add 50-message cap with FIFO eviction
10. Store `sessionId` per engagement in claudeStore
11. Modify `spawn_claude_session` to accept optional `resume_session_id` param
12. Implement resume-with-timeout logic (5s fallback to fresh)
13. Create `useWorkspaceSession` hook (unified Claude + MCP lifecycle)
14. Wire `EngagementSwitcher` to `useWorkspaceSession.switchEngagement()`
15. Implement JSON-based PID registry (`{app_data_dir}/session-registry.json`)
16. Implement `cleanup_orphans()` called from Tauri setup
17. Tests for chat history map operations
18. Tests for orphan cleanup logic (mock PID checks)

**Estimated: 18 tasks, fits in one session.**

## 5. RECOMMENDED PHASE 3b TASK LIST

1. Add `.mcp-config.json` generation to scaffold (extend `scaffold.rs`)
2. Add env var injection to `spawn()` in session_manager.rs
3. Pass `--mcp-config` flag to Claude CLI in spawn
4. Read Google OAuth token from keychain at spawn time, inject as env var
5. Refactor `useEngagement.ts` to remove app-side Google MCP spawning
6. Add `--strict-mcp-config` as opt-in setting
7. Implement offline check before spawn (network connectivity test)
8. Add retry button to InputBar when error state
9. Add mid-session error recovery UI state
10. Update scaffold tests for `.mcp-config.json` generation
11. Integration test: verify MCP config format matches Claude CLI expectations
12. Update spec section 3.9 to reflect final engagement switching design

**Estimated: 12 tasks, fits in one session.**

---

## 6. THINGS DONE WELL

1. **The Phase 1+2 foundation is solid.** The stream parser, session manager, and skill scaffold are production-quality code. The type system is well-defined on both Rust and TypeScript sides.

2. **The proposal identifies the right features.** Every item in the scope directly serves the consultant experience. No scope creep, no speculative infrastructure.

3. **The MCP question is correctly identified as the key decision.** Asking "should Claude CLI manage MCP servers or should the app?" is the right question at the right time.

4. **Build health is excellent.** Zero type errors, zero test failures, 34/34 tests passing. The 13 cargo warnings are all dead-code warnings from Phase 2 types not yet used in Phase 3 -- appropriate and expected.

---

## 7. CONDITIONS FOR PASS

This review is a **WARN**. To upgrade to **PASS** before execution begins:

- [ ] **W1:** Acknowledge C1 (session map leak) will be fixed as task 1 of Phase 3a
- [ ] **W2:** Acknowledge C2 (unified hook) -- confirm the `useWorkspaceSession` approach or propose alternative
- [ ] **W3:** Confirm Phase 3a/3b split (or justify single-phase with task count)
- [ ] **W4:** Decide on JSON file vs SQLite for PID registry (Codex recommends JSON)
- [ ] **W5:** Decide on `--strict-mcp-config` default (Codex recommends non-strict as default)

Address these in the phase plan. No code changes needed yet -- just design decisions confirmed in writing.

---

**Codex verdict: WARN 7/10 -- approve scope, split phases, fix C1 before executing.**
