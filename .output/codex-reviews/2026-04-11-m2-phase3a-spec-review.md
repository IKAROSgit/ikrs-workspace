# Codex Tier 2 Spec Review -- M2 Phase 3a: Session Management + UX Polish

**Reviewer:** Codex (Claude Opus 4.6)
**Date:** 2026-04-11
**Spec:** `docs/specs/m2-phase3a-session-ux-design.md` (commit `8b325e0`)
**Parent spec:** `docs/specs/embedded-claude-architecture.md` (sections 3.9-3.15)
**Prior reviews:** scope-review (WARN 7/10), design-assessment (PASS 8/10)

---

## VERDICT: WARN 7/10 -- 3 Critical issues, 4 Important issues, 3 Suggestions

The spec addresses all prior corrections (C1-C3) and advisories (A1-A4) correctly. The architecture is sound. However, three technical problems will cause implementation failures if not resolved before execution begins. The issues are fixable within the spec -- no architectural rethinking required.

---

## 1. STRUCTURAL VALIDATION

### 1.1 Internal consistency: MOSTLY CONSISTENT, two contradictions

**Contradiction #1: File change list for item 6 omits `EngagementSwitcher.tsx`.**
Section 6 says "Components that import `useEngagement` for switching -> import `useWorkspaceSession`." The only component that currently does this is `EngagementSwitcher.tsx` (at `/src/components/layout/EngagementSwitcher.tsx`, line 10: `import { useEngagement } from "@/hooks/useEngagement"`). This file is not listed in the "Files Changed" section. It must be listed explicitly -- a subagent will not infer it.

**Contradiction #2: `getResumeSessionId` is called in the hook code sketch but never defined.**
Section 6's `switchEngagement` code calls `getResumeSessionId(newEngagementId)` on line 339, but this function is never specified -- not in the IPC layer (section 5), not in `tauri-commands.ts`, not in the registry API. The spec defines `get_session_id()` in Rust but never exposes it as a Tauri command or IPC function.

### 1.2 File lists: ACCURATE otherwise

All other file lists match the described changes. The spec correctly identifies that `types.rs`, `stream_parser.rs`, `claude.ts`, `claudeStore.ts`, `useClaudeStream.ts`, and `ToolActivityCard.tsx` all need changes for item 2.

---

## 2. ARCHITECTURE ALIGNMENT WITH PARENT SPEC (3.9-3.15)

### 2.1 Section 3.9 (Session Management): ALIGNED

The parent spec describes the lifecycle: `spawn_session -> send_message -> kill_session`, engagement switching as `kill -> spawn with new cwd`, and session resume via `--resume {session_id}`. Phase 3a spec implements all three. The parent spec says "Store `session_id` per engagement in SQLite" -- the Phase 3a spec uses a JSON registry instead, which is a justified deviation (per the prior Codex review Q2 recommendation).

### 2.2 Section 3.11 (Frontend Components): ALIGNED

Parent spec says ToolActivityCard should be "Collapsed (default): icon + friendly label + spinner/checkmark" and SessionIndicator should have "Click to see: engagement name, session duration, token cost." Phase 3a implements both.

### 2.3 Section 3.14 (Process Health): ALIGNED

Parent spec describes orphan cleanup via PID tracking. Phase 3a implements this with JSON registry. The parent spec says "Store active session PIDs in SQLite on spawn" -- again, JSON is the justified deviation.

### 2.4 Section 3.15 (Offline): CORRECTLY DEFERRED

Offline behavior is deferred to Phase 3b. The parent spec's Phase 3 description bundles offline with MCP wiring. The 3a/3b split is architecturally correct -- offline detection requires network checks that are orthogonal to session management.

### 2.5 Deviation: Parent spec Phase 3 includes MCP wiring

The parent spec's "Phase 3: Polished UX + MCP" (line 957-963) includes "Per-engagement MCP config" and "Wire Gmail/Calendar/Drive MCP servers." These are correctly deferred to Phase 3b per the prior scope review. This is a beneficial deviation.

---

## 3. SECURITY AUDIT

### 3.1 JSON registry: LOW RISK

The registry at `{app_data_dir}/session-registry.json` stores engagement IDs, session IDs, PIDs, and timestamps. No credentials, no conversation content. The `app_data_dir` is OS-protected (`~/Library/Application Support/` on macOS, `~/.local/share/` on Linux). Risk is acceptable.

### 3.2 Orphan cleanup SIGTERM: MEDIUM RISK (adequately mitigated)

The spec includes process name verification before sending SIGTERM (section 7: "check that the process with the stored PID is actually a Claude process"). This mitigates PID reuse attacks. The `unsafe` block for `libc::kill` is appropriate for this use case.

### 3.3 Tool input/result forwarding: LOW RISK

The 4KB/2KB caps on `tool_input` and `result_content` prevent memory exhaustion from oversized tool payloads. The data stays within the Tauri IPC boundary (backend to webview) -- it does not leave the app process. No new external attack surface.

### 3.4 No new attack surfaces identified.

---

## 4. COMPLETENESS -- COVERAGE OF PRIOR ASSESSMENT ITEMS

| Item | Covered? | Notes |
|------|----------|-------|
| C1 (session map leak) | YES | Section 1, with correct fix |
| C2 (uncoordinated hooks) | YES | Section 6, `useWorkspaceSession` |
| C3 (tool_input discarded) | YES | Section 2, with 4KB cap |
| A1 (useEngagement retirement) | YES | Section 6, retirement path described |
| A2 (Obsidian MCP ownership) | NO | See Issue I1 below |
| A3 (tool_result_full) | YES | Section 2, `result_content` with 2KB cap |
| A4 (atomic write) | YES | Section 5, temp+rename pattern |

**Issue I1 (Important): A2 (Obsidian MCP) is not addressed.**

The design assessment A2 says: "Put Obsidian in `.mcp-config.json` alongside Google MCPs. Mark `McpProcessManager` for full removal in Phase 3b." The Phase 3a spec's "Out of Scope (Phase 3b)" section lists retiring `McpProcessManager` but says nothing about Obsidian MCP placement in `.mcp-config.json`. This is a Phase 3b concern, so deferral is acceptable, but the spec should at minimum acknowledge A2 and confirm it is tracked for Phase 3b. Currently it is silently dropped.

---

## 5. CRITICAL ISSUES (must fix before execution)

### CRITICAL-1: `historyCache` as `Map<string, ChatMessage[]>` breaks Zustand reactivity

**Severity:** CRITICAL -- will cause silent bugs in production

Zustand 5.x (confirmed version 5.0.12 in this project) uses shallow equality comparison (`Object.is()`) to determine if state has changed and subscribers should re-render. `Map` objects are compared by reference, not by content. When you call `historyCache.set(key, value)` in-place, Zustand does not detect the change because the Map reference has not changed.

Concretely, this code from the spec will NOT trigger re-renders:

```typescript
saveAndClearHistory: (engagementId) => set((state) => {
  state.historyCache.set(engagementId, state.messages.slice(-50));
  return { messages: [], historyCache: state.historyCache }; // SAME REF!
})
```

**Fix:** Replace `Map<string, ChatMessage[]>` with `Record<string, ChatMessage[]>`. This is a plain object -- Zustand handles it correctly with spread:

```typescript
interface ClaudeState {
  historyCache: Record<string, ChatMessage[]>;  // NOT Map
}

saveAndClearHistory: (engagementId) => set((state) => ({
  messages: [],
  engagementId: null,
  historyCache: {
    ...state.historyCache,
    [engagementId]: state.messages.slice(-50),
  },
}))
```

This change propagates to `loadHistory`, tests, and anywhere `historyCache` is accessed. Using `.get()` becomes bracket access `historyCache[id] ?? []`.

### CRITICAL-2: `monitor_process` cannot access the session map -- the spec's proposed fix is structurally incomplete

**Severity:** CRITICAL -- the C1 fix as described will not compile

The spec's section 1 shows:

```rust
let mut sessions = manager.sessions.lock().await;
sessions.remove(&session_id);
```

This implies `monitor_process` receives a `manager` parameter with access to `sessions`. But the current `monitor_process` signature is:

```rust
async fn monitor_process(mut child: Child, session_id: String, app: AppHandle)
```

And it is spawned in `spawn()` as:

```rust
tokio::spawn(async move {
    monitor_process(child, monitor_session_id, monitor_app).await;
});
```

The spec says "pass manager Arc into monitor" (section 1, line 70) but does not show the actual signature change or how to pass the `Arc<Mutex<HashMap>>`. The `ClaudeSessionManager.sessions` is already `Arc<Mutex<HashMap>>`, but `monitor_process` is a free function outside the struct -- it cannot access `self.sessions` directly.

**Fix:** The spec must specify the concrete signature change:

```rust
async fn monitor_process(
    mut child: Child,
    session_id: String,
    sessions: Arc<Mutex<HashMap<String, ClaudeSession>>>,
    app: AppHandle,
)
```

And the spawn site:

```rust
let monitor_sessions = self.sessions.clone();  // Arc::clone
tokio::spawn(async move {
    monitor_process(child, monitor_session_id, monitor_sessions, monitor_app).await;
});
```

This is not just a nit -- it changes the visibility of `ClaudeSession` (currently private to the module but needs to be accessible to the free function), and the spec's "Files Changed" section says only `session_manager.rs` changes, which is correct but the *nature* of the change needs to be explicit.

### CRITICAL-3: Resume fallback 5-second timeout mechanism is unspecified

**Severity:** CRITICAL -- ambiguous enough to cause incorrect implementation

The spec says (section 5, lines 276-278):

> 2. Spawn with 5-second timeout for session-ready event
> 3. If timeout or error: kill process, retry without `--resume` (new session)

But `spawn_claude_session` is an async Rust function that spawns a child process and returns immediately (the session_id). The `session-ready` event is emitted later by the stream parser when it receives the `init` system event from Claude CLI. The spawn function does not wait for `session-ready` -- it fires and forgets.

There is no mechanism in the current architecture for `spawn()` to wait for a Tauri event that is emitted by a different tokio task (the stream parser). The spec does not explain how to implement this timeout. Possible approaches:

**(A) tokio::sync::oneshot channel:** The spawn function creates a `oneshot::Sender`, passes it to the stream parser. When the parser sees `init`, it sends on the channel. The spawn function uses `tokio::time::timeout(Duration::from_secs(5), rx.await)`. This is the cleanest approach but requires threading the sender through `parse_stream()`.

**(B) Watch the store from Rust:** Not possible -- Zustand is frontend-only.

**(C) Poll for session-ready in Rust:** Add a flag to the session map entry (e.g., `is_ready: bool`) that the stream parser sets. The spawn function polls it with a loop + sleep. This is ugly but works.

**(D) Frontend-driven timeout:** Move the timeout logic to `useWorkspaceSession`. After calling `spawnClaudeSession()`, start a 5-second timer. If the store's `status` doesn't become `connected` within 5s, call `killClaudeSession()` and retry without resume. This is the simplest approach and keeps the Rust layer clean.

**Recommendation:** Option D. The spec should explicitly state that the resume timeout is handled in `useWorkspaceSession`, not in the Rust backend. The hook already orchestrates the full switching sequence -- adding a timeout there is natural.

---

## 6. IMPORTANT ISSUES (should fix)

### IMPORTANT-1: `useWorkspaceSession` hook has a stale closure over store state

The spec's code sketch (section 6) accesses stores via `claudeStore.getState()` and `engagementStore.getState()` inside an async function. This is actually the CORRECT pattern for Zustand (using `getState()` for imperative access outside of React render). However, the hook also subscribes to state via `useClaudeStore(s => s.status)` -- but `sessionStatus` from the subscription is never used in `switchEngagement`. This is dead code in the hook.

**Fix:** Either use `sessionStatus` in the hook (e.g., for the "Switching..." UI state) or remove the subscription. Currently it creates an unnecessary re-render on every status change.

### IMPORTANT-2: `saveAndClearHistory` clears `activeTools` along with messages -- but the spec does not mention it

When switching engagements, the spec says to save `messages` to `historyCache` and clear `messages`. But the `claudeStore` also has `activeTools: ToolActivity[]` which accumulates tool activity cards. If these are not cleared on engagement switch, the new session will show stale tool cards from the previous engagement.

**Fix:** `saveAndClearHistory` must also clear `activeTools` to `[]`. The spec should mention this explicitly. Additionally, consider whether `activeTools` should be part of the history cache (probably not -- tool activity is transient and not useful for history display).

### IMPORTANT-3: `cleanup_orphans` in `lib.rs` requires `setup()` -- currently absent

The spec says (section 7): "On app startup (`setup()` in `lib.rs`)" -- but the current `lib.rs` has no `setup()` function. The Tauri builder uses `.run()` directly without a `.setup()` callback.

The spec must show the actual integration point:

```rust
.setup(|app| {
    let app_data_dir = app.path().app_data_dir()
        .expect("Failed to get app data dir");
    cleanup_orphans(&app_data_dir);
    Ok(())
})
```

This is inserted before `.invoke_handler()` in the builder chain. The spec's "Files Changed" section lists `src-tauri/src/lib.rs` correctly but the change is not detailed.

### IMPORTANT-4: `libc::kill` is not cross-platform -- macOS + Linux support is incomplete

The spec says to use `unsafe { libc::kill(entry.pid as i32, libc::SIGTERM); }` for orphan cleanup. This works on macOS and Linux (both are POSIX), but:

1. The `libc` crate is not in `Cargo.toml` currently. It needs to be added.
2. The spec also says "On macOS: use `sysctl` or `proc_pidpath`" for process name verification and "On Linux: read `/proc/{pid}/cmdline`." These are two completely different implementations. The spec should either:
   - Specify using `sysinfo` crate (which abstracts both platforms), OR
   - Specify using `std::process::Command` with `ps -p {pid} -o comm=` (works on both macOS and Linux)

The `sysinfo` approach is cleaner. The `ps` approach requires no new dependencies. Either way, the spec must pick ONE approach and add the dependency to the "Files Changed" / Cargo.toml section.

**Recommendation:** Use `std::process::Command` with `ps -p {pid} -o comm=`. No new dependency. Works on both platforms. Falls back gracefully (if `ps` fails, skip the kill -- better to leave an orphan than kill the wrong process).

---

## 7. SUGGESTIONS (nice to have)

### SUGGESTION-1: Add "switching" status to `ClaudeSessionStatus`

The spec says SessionIndicator should show "Switching..." during engagement switches. Currently `ClaudeSessionStatus` is `"disconnected" | "connecting" | "connected" | "thinking" | "error"`. The spec does not add a `"switching"` variant. Instead, it uses a separate `switching` boolean in `useWorkspaceSession`.

This works but is inelegant -- the SessionIndicator would need to receive both `status` and `switching` props. Adding `"switching"` to the enum would be cleaner and keep all status in one place.

### SUGGESTION-2: Test file location inconsistency

The spec's testing strategy (section) says "Vitest + component" tests for ToolActivityCard and SessionDetailsModal, but the existing test structure places all tests in `/tests/unit/` (not co-located). The spec references `tests/unit/stores/claudeStore.test.ts` for the history tests, which matches the existing pattern. Component tests are not specified with a file path. Suggest adding explicit file paths for all test files:
- `tests/unit/stores/claudeStore.test.ts` (extend existing)
- `tests/unit/components/ToolActivityCard.test.tsx` (new)
- `tests/unit/components/SessionDetailsModal.test.tsx` (new)

### SUGGESTION-3: `sessionStartedAt` should be reset on disconnect

The spec adds `sessionStartedAt: number | null` to `claudeStore`, set on `setSessionReady`. But the `reset()` and `setDisconnected()` actions don't mention clearing this field. The `reset()` action currently resets to `initialState`, so `sessionStartedAt` should be in `initialState` as `null`. This is probably implied but should be explicit to avoid the subagent forgetting.

---

## 8. RISK REGISTER ASSESSMENT

### Existing risks: ADEQUATE

All 6 risks (P3a-R1 through P3a-R6) are correctly identified with appropriate severities and mitigations.

### Missing risk: P3a-R7

**P3a-R7: `--resume` flag unavailable or changed in future Claude CLI versions.**

The `--resume` flag is a Claude CLI feature, not a stable API. If Claude CLI changes or removes this flag, the resume feature silently fails. The current mitigation (5s timeout + fallback) handles runtime failure, but the spec should document this as a known version dependency and add it to the minimum version check.

**Severity:** Low. **Mitigation:** Already handled by the fallback mechanism, but should be documented.

---

## 9. SPEC ALIGNMENT SUMMARY

| Parent Spec Section | Phase 3a Coverage | Status |
|---------------------|-------------------|--------|
| 3.9 Session Management | Full | ALIGNED |
| 3.11 Frontend Components | ToolActivityCard + SessionIndicator | ALIGNED |
| 3.14 Process Health | Orphan cleanup | ALIGNED |
| 3.15 Offline Behavior | Deferred to 3b | JUSTIFIED DEFERRAL |
| MCP wiring | Deferred to 3b | JUSTIFIED DEFERRAL |

---

## 10. IMPLEMENTATION READINESS

**Can a subagent write a task-level plan from this spec?** ALMOST. The three critical issues must be resolved first:

1. **CRITICAL-1** (Map vs Record): Simple text substitution throughout the spec.
2. **CRITICAL-2** (monitor_process signature): Add the concrete Rust signature and spawn-site code.
3. **CRITICAL-3** (resume timeout): Specify that timeout lives in `useWorkspaceSession` (Option D), not in Rust.

After these fixes, the spec is execution-ready. The seven sections map cleanly to ~18 implementation tasks. The dependency chain (identified in the prior design assessment) is correct: C1 fix first, then UI items (parallel), then history + resume, then orchestrator, then orphan cleanup.

---

## 11. THINGS DONE WELL

1. **All prior Codex corrections are addressed.** C1, C2, C3 all have concrete solutions with code sketches. This is exactly how assessment feedback should be incorporated.

2. **The data flow diagram (section "Engagement Switch Sequence") is excellent.** It removes all ambiguity about the switching order. Every subagent implementing this will get it right.

3. **Size capping strategy (A3) is well-specified.** The 4KB/2KB caps with `chars().take()` and UTF-8 safety match the Phase 1 C2 fix pattern. Consistent approach across the codebase.

4. **The "Out of Scope" sections are clear and complete.** Both Phase 3b and Phase 4 boundaries are explicitly stated. No scope ambiguity.

5. **Atomic write for JSON registry (A4) is correct.** temp + rename is the standard pattern. The parse-error-to-empty-registry fallback is a good defensive choice.

---

## 12. CONDITIONS FOR PASS

This review is **WARN 7/10**. To upgrade to PASS:

- [ ] **W1:** Replace `Map<string, ChatMessage[]>` with `Record<string, ChatMessage[]>` throughout the spec (CRITICAL-1)
- [ ] **W2:** Add concrete `monitor_process` signature change and spawn-site code showing `Arc::clone` of sessions (CRITICAL-2)
- [ ] **W3:** Specify that resume timeout is frontend-driven (in `useWorkspaceSession`), not in Rust backend (CRITICAL-3)
- [ ] **W4:** Add `getResumeSessionId` to the IPC layer -- either as a Tauri command (needs Rust function + `commands.rs` + `tauri-commands.ts`) or as a frontend-only function that reads the registry via existing file access (Structural contradiction #2)
- [ ] **W5:** Add `EngagementSwitcher.tsx` to item 6's "Files Changed" list (Structural contradiction #1)
- [ ] **W6:** Clarify that `saveAndClearHistory` also clears `activeTools` (IMPORTANT-2)

Items W1-W3 are blocking. W4-W6 are strongly recommended.

---

**Codex verdict: WARN 7/10 -- sound architecture, three technical impossibilities need resolution before execution.**
