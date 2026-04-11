# M2 Phase 3a: Session Management + UX Polish

**Date:** 2026-04-11
**Status:** DESIGN
**Codex assessment:** PASS 8/10
**Prerequisite:** Phase 1 (embedded Claude) + Phase 2 (skill system) complete
**Spec sections:** 3.9 (Session Management), 3.11 (Frontend Components), 3.14 (Process Health), 3.15 (Offline)

## Overview

Phase 3a adds session lifecycle management and UI polish to the embedded Claude chat. After this phase, a consultant can switch between engagements without losing chat context, resume prior sessions, and inspect tool activity in detail.

Phase 3b (MCP wiring + resilience) follows as a separate phase.

## Scope

### In Scope

1. Fix session map leak on process exit (Codex C1 — pre-existing bug)
2. ToolActivityCard collapsible details with tool input and result content
3. SessionIndicator detail modal (engagement name, duration, cost, session_id)
4. Chat history partitioned by engagement (in-memory, 50-message FIFO)
5. Session resume via `--resume {session_id}` with JSON registry
6. Engagement switching orchestrator (`useWorkspaceSession` hook)
7. Orphan PID cleanup on app startup

### Out of Scope (Phase 3b)

- Per-engagement `.mcp-config.json` and `--mcp-config` flag
- Retiring `McpProcessManager` / app-side MCP spawning
- Gmail/Calendar/Drive MCP wiring
- Offline detection and retry UX
- `McpProcessManager` removal (deferred to 3b)

### Out of Scope (Phase 4)

- macOS App Sandbox, code signing, DMG packaging
- Security-Scoped Bookmarks
- Obsidian MCP in `.mcp-config.json`

---

## 1. Session Map Cleanup (Codex C1)

### Problem

`monitor_process` in `session_manager.rs` detects process exit and emits events but never removes the dead session from `self.sessions`. This means:
- `has_session()` returns true for dead processes
- `max_sessions` limit blocks spawning new sessions
- Engagement switching will fail silently

### Solution

After detecting process exit in `monitor_process`, remove the session from the HashMap before emitting the event. The `ClaudeSessionManager` needs to be passed into the monitor task so it can clean up.

```rust
// In monitor_process, after detecting exit:
{
    let mut sessions = manager.sessions.lock().await;
    sessions.remove(&session_id);
}
// THEN emit the event
let _ = app.emit("claude:session-ended", payload);
```

The `kill()` method already removes from the HashMap synchronously (line 162-169). The fix is making `monitor_process` do the same.

### Files Changed

- `src-tauri/src/claude/session_manager.rs` — pass manager Arc into monitor, remove session on exit

---

## 2. ToolActivityCard Collapsible Details (Codex C3)

### Problem

`stream_parser.rs` consumes raw tool input in `friendly_label()` and discards it. The frontend can only show "Editing settings.tsx" but not the actual edit parameters. Similarly, tool results are truncated to 80 chars.

### Solution

#### Backend: Forward tool data

Add two new fields to payloads:

```rust
// In ToolStartPayload
pub struct ToolStartPayload {
    pub tool_id: String,
    pub tool_name: String,
    pub friendly_label: String,
    pub tool_input: Option<String>,  // NEW: JSON-serialized input, 4KB cap
}

// In ToolEndPayload
pub struct ToolEndPayload {
    pub tool_id: String,
    pub success: bool,
    pub summary: String,
    pub result_content: Option<String>,  // NEW: Full result, 2KB cap
}
```

**Size capping strategy (A3):**
- Serialize `input` to JSON string via `serde_json::to_string()`
- If `s.chars().count() > 4096`, truncate to first 4093 chars + "..." (UTF-8 safe, same pattern as Phase 1 C2 fix)
- Store as `Option<String>` — frontend deserializes for display

For `result_content`, extract from `tool_result` content block:
- If string content, cap at 2048 chars using same `chars().take()` pattern
- If non-string content, serialize to JSON string with same cap

#### Frontend: Expandable card

```typescript
// Updated ToolActivity type
interface ToolActivity {
  toolId: string;
  toolName: string;
  label: string;
  status: "running" | "done" | "error";
  toolInput?: string;       // NEW
  resultContent?: string;   // NEW
}
```

ToolActivityCard behavior:
- **Collapsed (default):** Icon + friendly label + spinner/checkmark (current behavior)
- **Click to expand:** Shows formatted tool input parameters and result content
- Expand state is per-card, managed locally with `useState`
- Input displayed as syntax-highlighted JSON (or plain text fallback)
- Result displayed as plain text with monospace font
- Collapse on second click

### Files Changed

- `src-tauri/src/claude/types.rs` — add fields to payloads
- `src-tauri/src/claude/stream_parser.rs` — serialize and cap tool_input, extract result_content
- `src/types/claude.ts` — add fields to ToolActivity and event payloads
- `src/stores/claudeStore.ts` — store new fields in tool activity
- `src/hooks/useClaudeStream.ts` — forward new fields from events to store
- `src/components/chat/ToolActivityCard.tsx` — add expand/collapse UI

---

## 3. SessionIndicator Detail Modal

### Design

Click the SessionIndicator bar to open a lightweight overlay showing:

| Field | Source |
|-------|--------|
| Engagement name | `engagementStore.engagements.find(e => e.id === activeId)` |
| Client name | `engagementStore.clients.find(c => c.id === engagement.clientId)` |
| Session ID | `claudeStore.sessionId` |
| Status | `claudeStore.status` mapped to human label |
| Duration | Computed from `sessionStartedAt` timestamp (new field in claudeStore) |
| Total cost | `claudeStore.totalCostUsd` formatted as `$X.XX` |
| Model | `claudeStore.model` (from session-ready event) |

New `sessionStartedAt: number | null` field in claudeStore, set on `setSessionReady`.

### Component

New `SessionDetailsModal.tsx`:
- Positioned below SessionIndicator (dropdown, not center modal)
- Dismiss on click outside or Escape key
- No separate route — overlay on current view
- Tailwind styling consistent with existing cards

### Files Changed

- `src/stores/claudeStore.ts` — add `sessionStartedAt`, `model` fields
- `src/hooks/useClaudeStream.ts` — set model from session-ready event
- `src/components/chat/SessionDetailsModal.tsx` — NEW
- `src/components/chat/SessionIndicator.tsx` — add click handler, render modal

---

## 4. Chat History Per Engagement

### Problem

`claudeStore` holds messages in a flat array. When switching engagements, messages are lost (store resets).

### Solution

Add an in-memory `Map<string, ChatMessage[]>` to claudeStore for history partitioning.

```typescript
interface ClaudeState {
  // Existing
  messages: ChatMessage[];
  // ...

  // NEW
  engagementId: string | null;
  historyCache: Map<string, ChatMessage[]>;  // engagement_id → messages

  // NEW actions
  saveAndClearHistory: (engagementId: string) => void;
  loadHistory: (engagementId: string) => void;
}
```

**`saveAndClearHistory(engagementId)`:**
1. Copy current `messages` into `historyCache.get(engagementId)`
2. Apply FIFO cap: keep last 50 messages
3. Clear `messages` to empty array
4. Set `engagementId` to null

**`loadHistory(engagementId)`:**
1. Set `engagementId`
2. Set `messages` to `historyCache.get(engagementId) ?? []`

**FIFO cap:** 50 messages per engagement. When adding message 51, remove message 1. Applied on save, not on add (to avoid mid-conversation truncation).

**No disk persistence:** Chat history is session-scoped (in-memory only). Claude CLI persists full conversation in `~/.claude/`. The app's history cache is for UI convenience during engagement switching, not long-term storage.

### Files Changed

- `src/stores/claudeStore.ts` — add historyCache, engagementId, save/load actions
- `tests/unit/stores/claudeStore.test.ts` — add tests for save/load/FIFO

---

## 5. Session Resume

### JSON Registry

File: `{app_data_dir}/session-registry.json`

```json
{
  "sessions": {
    "engagement-id-abc": {
      "session_id": "sess_abc123",
      "pid": 12345,
      "started_at": "2026-04-11T10:00:00Z"
    }
  }
}
```

**Atomic write (A4):** Write to `session-registry.json.tmp`, then rename to `session-registry.json`. On read: if JSON parse fails, treat as empty registry (log warning, don't crash).

### Rust Backend

New functions in `session_manager.rs` (or new `registry.rs`):

```rust
pub fn load_registry(app_data_dir: &Path) -> SessionRegistry { ... }
pub fn save_registry(app_data_dir: &Path, registry: &SessionRegistry) { ... }
pub fn register_session(app_data_dir: &Path, engagement_id: &str, session_id: &str, pid: u32) { ... }
pub fn unregister_session(app_data_dir: &Path, engagement_id: &str) { ... }
pub fn get_session_id(app_data_dir: &Path, engagement_id: &str) -> Option<String> { ... }
```

### Spawn Modification

`spawn_claude_session` gets a new optional parameter:

```rust
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,  // NEW
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String>
```

If `resume_session_id` is `Some(id)`:
1. Add `--resume {id}` to CLI args
2. Spawn with 5-second timeout for session-ready event
3. If timeout or error: kill process, retry without `--resume` (new session)
4. Log resume failure but don't surface to user (seamless fallback)

On successful spawn: write to registry via `register_session()`.
On session end (normal or crash): clear via `unregister_session()`.

### Frontend

`useWorkspaceSession` (item 6) handles the resume logic. The IPC layer just accepts the optional param.

### Files Changed

- `src-tauri/src/claude/session_manager.rs` — accept resume param, add --resume flag
- `src-tauri/src/claude/registry.rs` — NEW: JSON registry CRUD
- `src-tauri/src/claude/mod.rs` — export registry
- `src-tauri/src/claude/commands.rs` — pass resume_session_id through IPC
- `src/lib/tauri-commands.ts` — update spawnClaudeSession signature

---

## 6. Engagement Switching Orchestrator

### The Problem (Codex C2)

Currently `useEngagement.ts` manages MCP lifecycle and `ChatView.tsx` manages Claude session lifecycle independently. When switching engagements, there is no coordination — race conditions occur.

### Solution: `useWorkspaceSession` Hook

Single hook that orchestrates the full switching sequence:

```typescript
function useWorkspaceSession() {
  // Consumes from both stores
  const activeEngagementId = useEngagementStore(s => s.activeEngagementId);
  const sessionStatus = useClaudeStore(s => s.status);

  // State
  const [switching, setSwitching] = useState(false);

  async function switchEngagement(newEngagementId: string): Promise<void> {
    if (switching) return;  // debounce
    setSwitching(true);

    try {
      // 1. Kill current Claude session (synchronous HashMap removal)
      if (claudeStore.getState().sessionId) {
        await killClaudeSession(claudeStore.getState().sessionId);
      }

      // 2. Save current chat history
      const currentId = engagementStore.getState().activeEngagementId;
      if (currentId) {
        claudeStore.getState().saveAndClearHistory(currentId);
      }

      // 3. Set new active engagement
      engagementStore.getState().setActiveEngagement(newEngagementId);

      // 4. Load target engagement's chat history
      claudeStore.getState().loadHistory(newEngagementId);

      // 5. Check for resume session
      const resumeId = await getResumeSessionId(newEngagementId);

      // 6. Spawn new Claude session
      const engagement = engagementStore.getState().engagements
        .find(e => e.id === newEngagementId);
      if (engagement) {
        await spawnClaudeSession(
          newEngagementId,
          engagement.vault.path,
          resumeId ?? undefined,
        );
      }
    } finally {
      setSwitching(false);
    }
  }

  return { switchEngagement, switching, sessionStatus };
}
```

### Retirement of `useEngagement` Switch Path (A1)

- `useEngagement.ts` keeps non-session concerns (loading engagements, creating clients)
- `useEngagement.switchEngagement()` is removed
- `EngagementSwitcher` component imports `useWorkspaceSession` for switching
- MCP spawning in `useEngagement.ts` stays until Phase 3b retires it

### UI During Switch

- `switching === true` → SessionIndicator shows "Switching..." with spinner
- InputBar disabled during switch
- Chat area shows previous engagement's messages fading out, new messages loading in
- If spawn fails, show error in SessionIndicator with retry button

### Files Changed

- `src/hooks/useWorkspaceSession.ts` — NEW
- `src/hooks/useEngagement.ts` — remove switchEngagement, remove MCP spawning logic related to Claude
- `src/views/ChatView.tsx` — use `useWorkspaceSession` instead of direct spawn/kill
- `src/components/chat/SessionIndicator.tsx` — show switching state
- `src/components/chat/InputBar.tsx` — disable during switching
- Components that import `useEngagement` for switching → import `useWorkspaceSession`

---

## 7. Orphan PID Cleanup

### Problem

If the app crashes or is force-quit, Claude CLI child processes become orphans. On next launch, they consume resources and may hold locks.

### Solution

Uses the same JSON registry from item 5.

On app startup (`setup()` in `lib.rs`):

```rust
fn cleanup_orphans(app_data_dir: &Path) {
    let registry = load_registry(app_data_dir);
    for (engagement_id, entry) in &registry.sessions {
        if is_process_alive(entry.pid) && is_claude_process(entry.pid) {
            // Send SIGTERM
            unsafe { libc::kill(entry.pid as i32, libc::SIGTERM); }
        }
    }
    // Clear all entries (fresh start)
    save_registry(app_data_dir, &SessionRegistry::default());
}
```

**Process name verification:** Before sending SIGTERM, check that the process with the stored PID is actually a Claude process (not a reused PID). On Linux: read `/proc/{pid}/cmdline` and check for "claude". On macOS: use `sysctl` or `proc_pidpath`.

**Cross-platform:** Use `sysinfo` crate (already common in Tauri apps) for process info, or `std::process::Command` with `ps -p {pid} -o comm=`.

### Files Changed

- `src-tauri/src/claude/registry.rs` — add `cleanup_orphans()` and `is_claude_process()`
- `src-tauri/src/lib.rs` — call `cleanup_orphans()` in `setup()`
- `Cargo.toml` — add `sysinfo` or `libc` dependency if needed

---

## Data Flow: Engagement Switch Sequence

```
User clicks engagement in sidebar
  │
  ▼
useWorkspaceSession.switchEngagement(newId)
  │
  ├── 1. killClaudeSession(currentSessionId)
  │      └── session_manager.kill() → removes from HashMap
  │      └── emits claude:session-ended
  │
  ├── 2. claudeStore.saveAndClearHistory(currentEngagementId)
  │      └── messages → historyCache[currentId] (FIFO 50)
  │      └── messages = []
  │
  ├── 3. engagementStore.setActiveEngagement(newId)
  │
  ├── 4. claudeStore.loadHistory(newId)
  │      └── messages = historyCache[newId] ?? []
  │
  ├── 5. registry.getSessionId(newId) → resumeId?
  │
  └── 6. spawnClaudeSession(newId, path, resumeId?)
         └── claude --print --resume {id}? ...
         └── registry.registerSession(newId, sessionId, pid)
         └── emits claude:session-ready
```

---

## Testing Strategy

| Item | Test Type | What |
|------|-----------|------|
| 1. Session map cleanup | Rust unit test | Verify session removed from HashMap after process exit |
| 2. Tool input/result capping | Rust unit test | Verify 4KB/2KB caps, UTF-8 safety |
| 2. ToolActivityCard expand | Vitest + component | Verify expand/collapse toggles, renders input/result |
| 3. SessionDetailsModal | Vitest + component | Verify fields rendered from store state |
| 4. Chat history | Vitest unit test | Verify save/load/FIFO behavior on claudeStore |
| 5. JSON registry | Rust unit test | Verify CRUD, atomic write, corrupt file handling |
| 5. Resume fallback | Rust unit test | Verify spawn without --resume on timeout |
| 6. useWorkspaceSession | Vitest integration | Verify switching sequence calls in order |
| 7. Orphan cleanup | Rust unit test | Verify dead PID cleanup, process name check |

---

## Risk Register

| ID | Risk | Severity | Mitigation |
|----|------|----------|------------|
| P3a-R1 | `--resume` with stale session_id | Medium | 5s timeout, fallback to new session |
| P3a-R2 | PID reuse (orphan cleanup kills wrong process) | Medium | Verify process name contains "claude" |
| P3a-R3 | Rapid engagement switching race condition | Medium | Debounce + disable UI during switch |
| P3a-R4 | JSON registry corruption on crash | Low | Atomic write (temp+rename), parse error → empty registry |
| P3a-R5 | `historyCache` memory unbounded | Low | 50-message FIFO per engagement, ~5KB per engagement |
| P3a-R6 | Monitor task cleanup races with `kill()` | Low | Both call `sessions.remove()` — HashMap remove is idempotent |
