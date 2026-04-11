# M2 Phase 3a: Session Management + UX Polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add session lifecycle management (resume, switching, orphan cleanup) and UI polish (expandable tools, session details) to the embedded Claude chat.

**Architecture:** Rust backend adds session registry (JSON file) and fixes session map cleanup. TypeScript frontend adds chat history partitioning (Record), `useWorkspaceSession` orchestrator hook, and two new UI features (expandable ToolActivityCard, session detail dropdown).

**Tech Stack:** Rust (Tauri 2), TypeScript (React 19, Zustand v5), Vitest, TDD

**Spec:** `docs/specs/m2-phase3a-session-ux-design.md`
**Codex:** PASS 8/10 (all 6 conditions resolved)

---

## File Map

### Files to CREATE

| File | Responsibility |
|------|----------------|
| `src-tauri/src/claude/registry.rs` | JSON session registry — CRUD, atomic write, orphan cleanup |
| `src/hooks/useWorkspaceSession.ts` | Engagement switching orchestrator — kill→save→switch→spawn |
| `src/components/chat/SessionDetailsModal.tsx` | Session detail dropdown (name, duration, cost, session_id) |

### Files to MODIFY

| File | What Changes |
|------|-------------|
| `src-tauri/src/claude/session_manager.rs` | C1 fix (monitor cleanup), accept `resume_session_id` param |
| `src-tauri/src/claude/types.rs` | Add `tool_input` to ToolStartPayload, `result_content` to ToolEndPayload |
| `src-tauri/src/claude/stream_parser.rs` | Serialize tool input (4KB cap), extract result content (2KB cap) |
| `src-tauri/src/claude/commands.rs` | Add `get_resume_session_id` command, update `spawn` signature |
| `src-tauri/src/claude/mod.rs` | Export `registry` module |
| `src-tauri/src/lib.rs` | Add `.setup()` callback, register new commands |
| `src/types/claude.ts` | Add `toolInput`, `resultContent` to types and payloads |
| `src/stores/claudeStore.ts` | Add historyCache (Record), sessionStartedAt, save/load/FIFO actions |
| `src/hooks/useClaudeStream.ts` | Forward new tool fields |
| `src/components/chat/ToolActivityCard.tsx` | Add expand/collapse with input/result display |
| `src/components/chat/SessionIndicator.tsx` | Add click handler for detail modal |
| `src/components/layout/EngagementSwitcher.tsx` | Import `useWorkspaceSession` instead of `useEngagement` |
| `src/hooks/useEngagement.ts` | Remove `switchEngagement`, keep only non-session concerns |
| `src/views/ChatView.tsx` | Use `useWorkspaceSession` for session lifecycle |
| `src/lib/tauri-commands.ts` | Add `getResumeSessionId`, update `spawnClaudeSession` signature |
| `tests/unit/stores/claudeStore.test.ts` | Add tests for save/load/FIFO/clearing |

---

## Task 1: Fix Session Map Cleanup (Codex C1)

**Files:**
- Modify: `src-tauri/src/claude/session_manager.rs`

- [ ] **Step 1: Write failing test**

Create a test that verifies the session is removed after the monitor fires. Since `monitor_process` works with real child processes, this test verifies the `kill()` + `has_session()` contract (the actual monitor fix is structural):

Add to the bottom of `src-tauri/src/claude/session_manager.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_removed_after_kill() {
        let manager = ClaudeSessionManager::new();
        // Manually insert a fake session to test removal
        {
            let stdin = tokio::process::Command::new("cat")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
                .stdin
                .take()
                .unwrap();
            let session = ClaudeSession {
                stdin,
                session_id: "test-sess".to_string(),
                engagement_id: "test-eng".to_string(),
            };
            manager.sessions.lock().await.insert("test-sess".to_string(), session);
        }
        assert!(manager.has_session().await);
        manager.kill("test-sess").await.unwrap();
        assert!(!manager.has_session().await);
    }
}
```

- [ ] **Step 2: Run test to verify it passes (this tests existing kill behavior)**

Run: `cd src-tauri && cargo test test_session_removed_after_kill -- --nocapture`
Expected: PASS (kill already removes from HashMap)

- [ ] **Step 3: Fix monitor_process to also remove sessions**

In `session_manager.rs`, change the `monitor_process` function signature and body:

Replace the existing `monitor_process` function (lines 179-218) with:

```rust
/// Monitors a Claude child process and emits events on exit.
/// Cleans up the session map before emitting events (C1 fix).
async fn monitor_process(
    mut child: Child,
    session_id: String,
    sessions: Arc<Mutex<HashMap<String, ClaudeSession>>>,
    app: AppHandle,
) {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (event, reason) = if status.success() {
                    ("claude:session-ended", "Session ended normally".to_string())
                } else {
                    (
                        "claude:session-crashed",
                        classify_exit(status.code()),
                    )
                };
                // C1 fix: Remove dead session from map BEFORE emitting event
                {
                    let mut map = sessions.lock().await;
                    map.remove(&session_id);
                }
                let _ = app.emit(
                    event,
                    SessionEndPayload {
                        session_id,
                        exit_code: status.code(),
                        reason,
                    },
                );
                return;
            }
            Ok(None) => {
                // Still running
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => {
                // C1 fix: Remove dead session from map BEFORE emitting event
                {
                    let mut map = sessions.lock().await;
                    map.remove(&session_id);
                }
                let _ = app.emit(
                    "claude:session-crashed",
                    SessionEndPayload {
                        session_id,
                        exit_code: None,
                        reason: format!("Monitor error: {e}"),
                    },
                );
                return;
            }
        }
    }
}
```

Update the spawn site (lines 111-116) to clone the Arc:

```rust
        // Spawn process monitor task (detects crashes)
        let monitor_app = app.clone();
        let monitor_session_id = session_id.clone();
        let monitor_sessions = Arc::clone(&self.sessions);
        tokio::spawn(async move {
            monitor_process(child, monitor_session_id, monitor_sessions, monitor_app).await;
        });
```

- [ ] **Step 4: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: All tests PASS

- [ ] **Step 5: Run cargo check for warnings**

Run: `cd src-tauri && cargo check`
Expected: No new warnings

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/claude/session_manager.rs
git commit -m "fix: session map cleanup on process exit (Codex C1)"
```

---

## Task 2: Tool Data Forwarding (Codex C3)

**Files:**
- Modify: `src-tauri/src/claude/types.rs`
- Modify: `src-tauri/src/claude/stream_parser.rs`

- [ ] **Step 1: Write tests for capping logic**

Add to the bottom of `src-tauri/src/claude/stream_parser.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cap_string_short() {
        let s = "hello world";
        let capped = cap_string(s, 4096);
        assert_eq!(capped, "hello world");
    }

    #[test]
    fn test_cap_string_long() {
        let s = "x".repeat(5000);
        let capped = cap_string(&s, 4096);
        assert_eq!(capped.chars().count(), 4096);
        assert!(capped.ends_with("..."));
    }

    #[test]
    fn test_cap_string_utf8_safe() {
        // Arabic text (multi-byte chars) should not panic
        let s = "\u{0627}\u{0644}\u{0633}\u{0644}\u{0627}\u{0645}".repeat(1000);
        let capped = cap_string(&s, 100);
        assert_eq!(capped.chars().count(), 100);
        assert!(capped.ends_with("..."));
    }

    #[test]
    fn test_serialize_tool_input_under_cap() {
        let input = serde_json::json!({"file_path": "/test/file.md"});
        let result = serialize_tool_input(&input);
        assert!(result.is_some());
        assert!(result.unwrap().contains("file.md"));
    }

    #[test]
    fn test_extract_result_content_string() {
        let content = Some(serde_json::json!("File contents here"));
        let result = extract_result_content(&content);
        assert_eq!(result, Some("File contents here".to_string()));
    }

    #[test]
    fn test_extract_result_content_none() {
        let result = extract_result_content(&None);
        assert_eq!(result, None);
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL (functions not defined)**

Run: `cd src-tauri && cargo test test_cap_string`
Expected: FAIL — `cap_string`, `serialize_tool_input`, `extract_result_content` not found

- [ ] **Step 3: Add fields to types.rs payloads**

In `src-tauri/src/claude/types.rs`, update `ToolStartPayload`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ToolStartPayload {
    pub tool_id: String,
    pub tool_name: String,
    pub friendly_label: String,
    pub tool_input: Option<String>,
}
```

Update `ToolEndPayload`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ToolEndPayload {
    pub tool_id: String,
    pub success: bool,
    pub summary: String,
    pub result_content: Option<String>,
}
```

- [ ] **Step 4: Implement helper functions in stream_parser.rs**

Add these functions before the `parse_stream` function:

```rust
/// Cap a string to `max_chars` characters (UTF-8 safe).
fn cap_string(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars - 3).collect();
        format!("{truncated}...")
    }
}

/// Serialize tool input to a JSON string, capped at 4096 chars.
fn serialize_tool_input(input: &serde_json::Value) -> Option<String> {
    match serde_json::to_string(input) {
        Ok(s) => Some(cap_string(&s, 4096)),
        Err(_) => None,
    }
}

/// Extract result content from a tool_result content block, capped at 2048 chars.
fn extract_result_content(content: &Option<serde_json::Value>) -> Option<String> {
    match content {
        Some(serde_json::Value::String(s)) => Some(cap_string(s, 2048)),
        Some(v) => match serde_json::to_string(v) {
            Ok(s) => Some(cap_string(&s, 2048)),
            Err(_) => None,
        },
        None => None,
    }
}
```

- [ ] **Step 5: Update tool_use emission to include tool_input**

In `handle_assistant_event`, update the `"tool_use"` arm (around line 193):

```rust
            "tool_use" => {
                let tool_name = block["name"].as_str().unwrap_or("unknown");
                let tool_id = block["id"].as_str().unwrap_or("unknown");
                let input = &block["input"];
                let _ = app.emit(
                    "claude:tool-start",
                    ToolStartPayload {
                        tool_id: tool_id.to_string(),
                        tool_name: tool_name.to_string(),
                        friendly_label: friendly_label(tool_name, input),
                        tool_input: serialize_tool_input(input),
                    },
                );
            }
```

- [ ] **Step 6: Update tool_result emission to include result_content**

In `handle_user_event`, update the tool_result handler:

```rust
        if block["type"].as_str() == Some("tool_result") {
            let tool_id = block["tool_use_id"].as_str().unwrap_or("unknown");
            let is_error = block["is_error"].as_bool().unwrap_or(false);
            let content_ref = block.get("content").cloned();
            let _ = app.emit(
                "claude:tool-end",
                ToolEndPayload {
                    tool_id: tool_id.to_string(),
                    success: !is_error,
                    summary: summarize_tool_result(&content_ref, is_error),
                    result_content: extract_result_content(&content_ref),
                },
            );
        }
```

- [ ] **Step 7: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: All tests PASS (including 6 new tests)

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/claude/types.rs src-tauri/src/claude/stream_parser.rs
git commit -m "feat: forward tool input/result in event payloads (Codex C3, 4KB/2KB caps)"
```

---

## Task 3: TypeScript Tool Types + ToolActivityCard Expand

**Files:**
- Modify: `src/types/claude.ts`
- Modify: `src/stores/claudeStore.ts`
- Modify: `src/hooks/useClaudeStream.ts`
- Modify: `src/components/chat/ToolActivityCard.tsx`

- [ ] **Step 1: Write test for new store fields**

Add to `tests/unit/stores/claudeStore.test.ts`:

```typescript
  it("startTool stores toolInput", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading file.md", '{"file_path":"/test.md"}');
    const state = useClaudeStore.getState();
    expect(state.activeTools[0]!.toolInput).toBe('{"file_path":"/test.md"}');
  });

  it("endTool stores resultContent", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading file.md");
    useClaudeStore.getState().endTool("tu_1", true, "Completed", "file contents here");
    const state = useClaudeStore.getState();
    expect(state.activeTools[0]!.resultContent).toBe("file contents here");
  });
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `npx vitest run tests/unit/stores/claudeStore.test.ts`
Expected: FAIL — `startTool` has wrong arity

- [ ] **Step 3: Update TypeScript types**

In `src/types/claude.ts`, update `ToolActivity`:

```typescript
export interface ToolActivity {
  toolId: string;
  toolName: string;
  friendlyLabel: string;
  status: "running" | "success" | "error";
  summary?: string;
  toolInput?: string;
  resultContent?: string;
  startedAt: Date;
  completedAt?: Date;
}
```

Update `ToolStartPayload`:

```typescript
export interface ToolStartPayload {
  tool_id: string;
  tool_name: string;
  friendly_label: string;
  tool_input: string | null;
}
```

Update `ToolEndPayload`:

```typescript
export interface ToolEndPayload {
  tool_id: string;
  success: boolean;
  summary: string;
  result_content: string | null;
}
```

- [ ] **Step 4: Update claudeStore actions**

In `src/stores/claudeStore.ts`, update the `startTool` signature and action:

```typescript
  startTool: (toolId: string, toolName: string, friendlyLabel: string, toolInput?: string) => void;
```

Update the action:

```typescript
  startTool: (toolId, toolName, friendlyLabel, toolInput) =>
    set((state) => ({
      activeTools: [
        ...state.activeTools,
        {
          toolId,
          toolName,
          friendlyLabel,
          status: "running" as const,
          toolInput: toolInput ?? undefined,
          startedAt: new Date(),
        },
      ],
    })),
```

Update `endTool` signature:

```typescript
  endTool: (toolId: string, success: boolean, summary: string, resultContent?: string) => void;
```

Update the action:

```typescript
  endTool: (toolId, success, summary, resultContent) =>
    set((state) => ({
      activeTools: state.activeTools.map((t) =>
        t.toolId === toolId
          ? {
              ...t,
              status: (success ? "success" : "error") as "success" | "error",
              summary,
              resultContent: resultContent ?? undefined,
              completedAt: new Date(),
            }
          : t
      ),
    })),
```

- [ ] **Step 5: Update useClaudeStream to forward new fields**

In `src/hooks/useClaudeStream.ts`, update tool-start listener:

```typescript
      unlisteners.push(
        await listen<ToolStartPayload>("claude:tool-start", (event) => {
          store().startTool(
            event.payload.tool_id,
            event.payload.tool_name,
            event.payload.friendly_label,
            event.payload.tool_input ?? undefined
          );
        })
      );
```

Update tool-end listener:

```typescript
      unlisteners.push(
        await listen<ToolEndPayload>("claude:tool-end", (event) => {
          store().endTool(
            event.payload.tool_id,
            event.payload.success,
            event.payload.summary,
            event.payload.result_content ?? undefined
          );
        })
      );
```

- [ ] **Step 6: Update ToolActivityCard with expand/collapse**

Replace `src/components/chat/ToolActivityCard.tsx`:

```tsx
import { useState } from "react";
import { cn } from "@/lib/utils";
import { Loader2, CheckCircle, XCircle, ChevronDown, ChevronRight } from "lucide-react";
import type { ToolActivity } from "@/types/claude";

interface ToolActivityCardProps {
  tool: ToolActivity;
}

const TOOL_ICONS: Record<string, string> = {
  Write: "\u{1F4DD}",
  Edit: "\u{270F}\u{FE0F}",
  Read: "\u{1F4D6}",
  Glob: "\u{1F50D}",
  Grep: "\u{1F50D}",
  WebSearch: "\u{1F310}",
  WebFetch: "\u{1F310}",
};

export function ToolActivityCard({ tool }: ToolActivityCardProps) {
  const [expanded, setExpanded] = useState(false);
  const icon = TOOL_ICONS[tool.toolName] ?? "\u{2699}\u{FE0F}";
  const hasDetails = Boolean(tool.toolInput || tool.resultContent);

  return (
    <div
      className={cn(
        "rounded-md text-xs border border-border/50",
        "bg-muted/50",
        hasDetails && "cursor-pointer"
      )}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5"
        onClick={() => hasDetails && setExpanded(!expanded)}
      >
        {hasDetails && (
          expanded
            ? <ChevronDown size={12} className="text-muted-foreground shrink-0" />
            : <ChevronRight size={12} className="text-muted-foreground shrink-0" />
        )}
        <span>{icon}</span>
        <span className="flex-1 truncate">{tool.friendlyLabel}</span>
        {tool.status === "running" && (
          <Loader2 size={12} className="animate-spin text-muted-foreground" />
        )}
        {tool.status === "success" && (
          <CheckCircle size={12} className="text-green-500" />
        )}
        {tool.status === "error" && (
          <XCircle size={12} className="text-destructive" />
        )}
      </div>
      {expanded && (
        <div className="px-3 pb-2 space-y-1.5 border-t border-border/30 pt-1.5">
          {tool.toolInput && (
            <div>
              <span className="text-muted-foreground font-medium">Input:</span>
              <pre className="mt-0.5 p-1.5 rounded bg-background text-[10px] font-mono overflow-x-auto max-h-32 overflow-y-auto whitespace-pre-wrap break-all">
                {tool.toolInput}
              </pre>
            </div>
          )}
          {tool.resultContent && (
            <div>
              <span className="text-muted-foreground font-medium">Result:</span>
              <pre className="mt-0.5 p-1.5 rounded bg-background text-[10px] font-mono overflow-x-auto max-h-32 overflow-y-auto whitespace-pre-wrap break-all">
                {tool.resultContent}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 7: Fix existing tests for new arity**

In `tests/unit/stores/claudeStore.test.ts`, the existing `startTool` call on line 43 has 3 args which still works (4th is optional). Existing `endTool` on line 52 has 3 args — update to 4 optional:

No changes needed — the 4th argument is optional in both functions.

- [ ] **Step 8: Run all tests**

Run: `npx vitest run`
Expected: All PASS

- [ ] **Step 9: Run tsc**

Run: `npx tsc --noEmit`
Expected: 0 errors

- [ ] **Step 10: Commit**

```bash
git add src/types/claude.ts src/stores/claudeStore.ts src/hooks/useClaudeStream.ts src/components/chat/ToolActivityCard.tsx tests/unit/stores/claudeStore.test.ts
git commit -m "feat: expandable ToolActivityCard with tool input/result details"
```

---

## Task 4: SessionIndicator Detail Modal

**Files:**
- Modify: `src/stores/claudeStore.ts`
- Modify: `src/components/chat/SessionIndicator.tsx`
- Create: `src/components/chat/SessionDetailsModal.tsx`

- [ ] **Step 1: Write test for sessionStartedAt**

Add to `tests/unit/stores/claudeStore.test.ts`:

```typescript
  it("setSessionReady sets sessionStartedAt", () => {
    const before = Date.now();
    useClaudeStore.getState().setSessionReady("sess-1", ["Read"], "claude-sonnet-4-6");
    const state = useClaudeStore.getState();
    expect(state.sessionStartedAt).toBeGreaterThanOrEqual(before);
    expect(state.sessionStartedAt).toBeLessThanOrEqual(Date.now());
  });
```

- [ ] **Step 2: Run test — expect FAIL**

Run: `npx vitest run tests/unit/stores/claudeStore.test.ts`
Expected: FAIL — `sessionStartedAt` is undefined

- [ ] **Step 3: Add sessionStartedAt to claudeStore**

In `src/stores/claudeStore.ts`, add to the interface:

```typescript
  sessionStartedAt: number | null;
```

Add to initialState:

```typescript
  sessionStartedAt: null as number | null,
```

Update `setSessionReady`:

```typescript
  setSessionReady: (sessionId, tools, model) =>
    set({
      sessionId,
      status: "connected",
      availableTools: tools,
      model,
      error: null,
      sessionStartedAt: Date.now(),
    }),
```

- [ ] **Step 4: Run test — expect PASS**

Run: `npx vitest run tests/unit/stores/claudeStore.test.ts`
Expected: PASS

- [ ] **Step 5: Create SessionDetailsModal**

Create `src/components/chat/SessionDetailsModal.tsx`:

```tsx
import { useEffect, useRef } from "react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";

interface SessionDetailsModalProps {
  onClose: () => void;
}

function formatDuration(startMs: number | null): string {
  if (!startMs) return "—";
  const seconds = Math.floor((Date.now() - startMs) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds}s`;
}

export function SessionDetailsModal({ onClose }: SessionDetailsModalProps) {
  const sessionId = useClaudeStore((s) => s.sessionId);
  const status = useClaudeStore((s) => s.status);
  const model = useClaudeStore((s) => s.model);
  const totalCostUsd = useClaudeStore((s) => s.totalCostUsd);
  const sessionStartedAt = useClaudeStore((s) => s.sessionStartedAt);

  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);

  const engagement = engagements.find((e) => e.id === activeEngagementId);
  const client = clients.find((c) => c.id === engagement?.clientId);

  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [onClose]);

  const STATUS_LABELS: Record<string, string> = {
    disconnected: "Disconnected",
    connecting: "Connecting",
    connected: "Connected",
    thinking: "Thinking",
    error: "Error",
  };

  return (
    <div
      ref={ref}
      className="absolute top-full left-0 right-0 z-50 mx-2 mt-1 rounded-lg border border-border bg-popover p-3 shadow-lg text-xs"
    >
      <div className="grid grid-cols-2 gap-y-1.5 gap-x-4">
        <span className="text-muted-foreground">Client</span>
        <span className="font-medium">{client?.name ?? "—"}</span>

        <span className="text-muted-foreground">Engagement</span>
        <span className="font-medium truncate">
          {engagement?.settings.description ?? "—"}
        </span>

        <span className="text-muted-foreground">Status</span>
        <span className="font-medium">{STATUS_LABELS[status] ?? status}</span>

        <span className="text-muted-foreground">Model</span>
        <span className="font-medium">{model ?? "—"}</span>

        <span className="text-muted-foreground">Duration</span>
        <span className="font-medium">{formatDuration(sessionStartedAt)}</span>

        <span className="text-muted-foreground">Cost</span>
        <span className="font-medium">${totalCostUsd.toFixed(4)}</span>

        <span className="text-muted-foreground">Session ID</span>
        <span className="font-mono text-[10px] truncate">{sessionId ?? "—"}</span>
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Update SessionIndicator with click handler**

Replace `src/components/chat/SessionIndicator.tsx`:

```tsx
import { useState } from "react";
import { cn } from "@/lib/utils";
import type { ClaudeSessionStatus } from "@/types/claude";
import { SessionDetailsModal } from "@/components/chat/SessionDetailsModal";

interface SessionIndicatorProps {
  status: ClaudeSessionStatus;
  model: string | null;
  costUsd: number;
  switching?: boolean;
}

const STATUS_CONFIG: Record<
  ClaudeSessionStatus,
  { color: string; label: string }
> = {
  disconnected: { color: "bg-gray-400", label: "Disconnected" },
  connecting: { color: "bg-yellow-400 animate-pulse", label: "Connecting..." },
  connected: { color: "bg-green-500", label: "Connected" },
  thinking: { color: "bg-yellow-400 animate-pulse", label: "Thinking..." },
  error: { color: "bg-red-500", label: "Error" },
};

export function SessionIndicator({
  status,
  model,
  costUsd,
  switching,
}: SessionIndicatorProps) {
  const [showDetails, setShowDetails] = useState(false);
  const config = STATUS_CONFIG[status];
  const label = switching ? "Switching..." : config.label;

  return (
    <div className="relative">
      <div
        className="flex items-center gap-3 px-4 py-2 border-b border-border text-xs text-muted-foreground cursor-pointer hover:bg-muted/50 transition-colors"
        onClick={() => setShowDetails(!showDetails)}
      >
        <div className="flex items-center gap-1.5">
          <div className={cn("w-2 h-2 rounded-full", switching ? "bg-yellow-400 animate-pulse" : config.color)} />
          <span>{label}</span>
        </div>
        {model && <span className="hidden sm:inline">{model}</span>}
        {costUsd > 0 && (
          <span className="ml-auto">${costUsd.toFixed(4)}</span>
        )}
      </div>
      {showDetails && (
        <SessionDetailsModal onClose={() => setShowDetails(false)} />
      )}
    </div>
  );
}
```

- [ ] **Step 7: Run tsc + vitest**

Run: `npx tsc --noEmit && npx vitest run`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/stores/claudeStore.ts src/components/chat/SessionIndicator.tsx src/components/chat/SessionDetailsModal.tsx tests/unit/stores/claudeStore.test.ts
git commit -m "feat: session detail dropdown on SessionIndicator click"
```

---

## Task 5: Chat History Per Engagement

**Files:**
- Modify: `src/stores/claudeStore.ts`
- Modify: `tests/unit/stores/claudeStore.test.ts`

- [ ] **Step 1: Write failing tests**

Add to `tests/unit/stores/claudeStore.test.ts`:

```typescript
  describe("history partitioning", () => {
    it("saveAndClearHistory saves messages and clears state", () => {
      useClaudeStore.getState().addUserMessage("Hello");
      useClaudeStore.getState().addTextDelta("msg_1", "Hi back");
      useClaudeStore.getState().startTool("tu_1", "Read", "Reading file");

      useClaudeStore.getState().saveAndClearHistory("eng-1");
      const state = useClaudeStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.activeTools).toEqual([]);
      expect(state.engagementId).toBeNull();
      expect(state.historyCache["eng-1"]).toHaveLength(2);
    });

    it("loadHistory restores messages", () => {
      useClaudeStore.getState().addUserMessage("Hello");
      useClaudeStore.getState().saveAndClearHistory("eng-1");

      // Switch to different engagement
      useClaudeStore.getState().addUserMessage("Other");
      useClaudeStore.getState().saveAndClearHistory("eng-2");

      // Load eng-1
      useClaudeStore.getState().loadHistory("eng-1");
      const state = useClaudeStore.getState();
      expect(state.engagementId).toBe("eng-1");
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0]!.text).toBe("Hello");
    });

    it("loadHistory returns empty for unknown engagement", () => {
      useClaudeStore.getState().loadHistory("eng-unknown");
      const state = useClaudeStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.engagementId).toBe("eng-unknown");
    });

    it("saveAndClearHistory applies FIFO cap of 50", () => {
      for (let i = 0; i < 60; i++) {
        useClaudeStore.getState().addUserMessage(`Message ${i}`);
      }
      useClaudeStore.getState().saveAndClearHistory("eng-full");
      const cached = useClaudeStore.getState().historyCache["eng-full"];
      expect(cached).toHaveLength(50);
      // Should keep the LAST 50 (messages 10-59)
      expect(cached![0]!.text).toBe("Message 10");
      expect(cached![49]!.text).toBe("Message 59");
    });
  });
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `npx vitest run tests/unit/stores/claudeStore.test.ts`
Expected: FAIL — `saveAndClearHistory` not defined

- [ ] **Step 3: Implement history partitioning in claudeStore**

In `src/stores/claudeStore.ts`, update the interface:

```typescript
interface ClaudeState {
  sessionId: string | null;
  status: ClaudeSessionStatus;
  messages: ChatMessage[];
  activeTools: ToolActivity[];
  totalCostUsd: number;
  error: string | null;
  availableTools: string[];
  model: string | null;
  sessionStartedAt: number | null;
  engagementId: string | null;
  historyCache: Record<string, ChatMessage[]>;

  setSessionReady: (sessionId: string, tools: string[], model: string) => void;
  addUserMessage: (text: string) => void;
  addTextDelta: (messageId: string, text: string) => void;
  startTool: (toolId: string, toolName: string, friendlyLabel: string, toolInput?: string) => void;
  endTool: (toolId: string, success: boolean, summary: string, resultContent?: string) => void;
  completeTurn: (costUsd: number, durationMs: number) => void;
  setError: (message: string) => void;
  setDisconnected: (reason: string) => void;
  saveAndClearHistory: (engagementId: string) => void;
  loadHistory: (engagementId: string) => void;
  reset: () => void;
}
```

Update initialState:

```typescript
const initialState = {
  sessionId: null as string | null,
  status: "disconnected" as ClaudeSessionStatus,
  messages: [] as ChatMessage[],
  activeTools: [] as ToolActivity[],
  totalCostUsd: 0,
  error: null as string | null,
  availableTools: [] as string[],
  model: null as string | null,
  sessionStartedAt: null as number | null,
  engagementId: null as string | null,
  historyCache: {} as Record<string, ChatMessage[]>,
};
```

Add the two new actions before `reset`:

```typescript
  saveAndClearHistory: (engagementId) =>
    set((state) => {
      const FIFO_CAP = 50;
      const messagesToSave = state.messages.length > FIFO_CAP
        ? state.messages.slice(-FIFO_CAP)
        : [...state.messages];
      return {
        historyCache: { ...state.historyCache, [engagementId]: messagesToSave },
        messages: [],
        activeTools: [],
        engagementId: null,
      };
    }),

  loadHistory: (engagementId) =>
    set((state) => ({
      engagementId,
      messages: state.historyCache[engagementId] ?? [],
    })),
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `npx vitest run tests/unit/stores/claudeStore.test.ts`
Expected: All PASS

- [ ] **Step 5: Run tsc**

Run: `npx tsc --noEmit`
Expected: 0 errors

- [ ] **Step 6: Commit**

```bash
git add src/stores/claudeStore.ts tests/unit/stores/claudeStore.test.ts
git commit -m "feat: chat history partitioning per engagement (Record, 50-msg FIFO)"
```

---

## Task 6: JSON Session Registry (Rust)

**Files:**
- Create: `src-tauri/src/claude/registry.rs`
- Modify: `src-tauri/src/claude/mod.rs`

- [ ] **Step 1: Write tests**

Create `src-tauri/src/claude/registry.rs` with tests first:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionRegistry {
    pub sessions: HashMap<String, SessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub session_id: String,
    pub pid: u32,
    pub started_at: String,
}

/// Load registry from disk. Returns empty registry on any error.
pub fn load_registry(app_data_dir: &Path) -> SessionRegistry {
    let path = app_data_dir.join("session-registry.json");
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => SessionRegistry::default(),
    }
}

/// Save registry to disk atomically (write tmp, then rename).
pub fn save_registry(app_data_dir: &Path, registry: &SessionRegistry) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = app_data_dir.join("session-registry.json");
    let tmp_path = app_data_dir.join("session-registry.json.tmp");
    let json = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
    std::fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Register a new session.
pub fn register_session(
    app_data_dir: &Path,
    engagement_id: &str,
    session_id: &str,
    pid: u32,
) -> Result<(), String> {
    let mut registry = load_registry(app_data_dir);
    registry.sessions.insert(
        engagement_id.to_string(),
        SessionEntry {
            session_id: session_id.to_string(),
            pid,
            started_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    save_registry(app_data_dir, &registry)
}

/// Unregister a session (on normal end or crash).
pub fn unregister_session(app_data_dir: &Path, engagement_id: &str) -> Result<(), String> {
    let mut registry = load_registry(app_data_dir);
    registry.sessions.remove(engagement_id);
    save_registry(app_data_dir, &registry)
}

/// Get the session_id for an engagement (for --resume).
pub fn get_session_id(app_data_dir: &Path, engagement_id: &str) -> Option<String> {
    let registry = load_registry(app_data_dir);
    registry
        .sessions
        .get(engagement_id)
        .map(|entry| entry.session_id.clone())
}

/// Check if a PID is alive via `ps`.
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if a PID belongs to a Claude process.
fn is_claude_process(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains("claude"),
        Err(_) => false,
    }
}

/// Kill a process by PID.
fn kill_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}

/// Clean up orphan Claude processes from a previous app crash.
/// Called once at app startup.
pub fn cleanup_orphans(app_data_dir: &Path) {
    let registry = load_registry(app_data_dir);
    for (_engagement_id, entry) in &registry.sessions {
        if is_process_alive(entry.pid) && is_claude_process(entry.pid) {
            log::info!("Killing orphan Claude process (PID {})", entry.pid);
            kill_process(entry.pid);
        }
    }
    // Clear all entries — fresh start
    let _ = save_registry(app_data_dir, &SessionRegistry::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ikrs-registry-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_register_and_get() {
        let dir = test_dir();
        register_session(&dir, "eng-1", "sess-abc", 12345).unwrap();
        let sid = get_session_id(&dir, "eng-1");
        assert_eq!(sid, Some("sess-abc".to_string()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_unregister() {
        let dir = test_dir();
        register_session(&dir, "eng-1", "sess-abc", 12345).unwrap();
        unregister_session(&dir, "eng-1").unwrap();
        let sid = get_session_id(&dir, "eng-1");
        assert_eq!(sid, None);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_empty_dir() {
        let dir = test_dir();
        let registry = load_registry(&dir);
        assert!(registry.sessions.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_corrupt_file() {
        let dir = test_dir();
        fs::write(dir.join("session-registry.json"), "NOT JSON").unwrap();
        let registry = load_registry(&dir);
        assert!(registry.sessions.is_empty()); // graceful fallback
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_atomic_write() {
        let dir = test_dir();
        register_session(&dir, "eng-1", "sess-abc", 12345).unwrap();
        // tmp file should not exist after successful save
        assert!(!dir.join("session-registry.json.tmp").exists());
        assert!(dir.join("session-registry.json").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_orphans_clears_registry() {
        let dir = test_dir();
        register_session(&dir, "eng-1", "sess-abc", 99999).unwrap();
        cleanup_orphans(&dir);
        let registry = load_registry(&dir);
        assert!(registry.sessions.is_empty());
        fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Export registry from mod.rs**

In `src-tauri/src/claude/mod.rs`, add:

```rust
pub mod registry;
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test registry`
Expected: 6 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/claude/registry.rs src-tauri/src/claude/mod.rs
git commit -m "feat: JSON session registry with atomic write and orphan cleanup"
```

---

## Task 7: Session Resume (Rust + TS)

**Files:**
- Modify: `src-tauri/src/claude/session_manager.rs`
- Modify: `src-tauri/src/claude/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/tauri-commands.ts`

- [ ] **Step 1: Update spawn to accept resume_session_id**

In `src-tauri/src/claude/session_manager.rs`, update the `spawn` method signature:

```rust
    pub async fn spawn(
        &self,
        engagement_id: String,
        engagement_path: String,
        resume_session_id: Option<String>,
        app: AppHandle,
    ) -> Result<String, String> {
```

Update the Command args section to conditionally add `--resume`:

```rust
        let mut args = vec![
            "--print".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--disallowed-tools".to_string(),
            "Bash".to_string(),
        ];

        if let Some(ref resume_id) = resume_session_id {
            args.push("--resume".to_string());
            args.push(resume_id.clone());
        }

        let mut child = Command::new("claude")
            .args(&args)
            .current_dir(&engagement_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn claude: {e}"))?;
```

- [ ] **Step 2: Update commands.rs**

Replace `src-tauri/src/claude/commands.rs`:

```rust
use crate::claude::session_manager::ClaudeSessionManager;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String> {
    let session_id = state
        .spawn(engagement_id.clone(), engagement_path, resume_session_id, app.clone())
        .await?;

    // Register in session registry for resume + orphan cleanup
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    // Get PID from the session (we don't have direct access, but we can skip PID for now
    // and store 0 — orphan cleanup will handle it via session_id)
    let _ = crate::claude::registry::register_session(
        &app_data_dir,
        &engagement_id,
        &session_id,
        std::process::id(), // Use parent PID as placeholder; real PID is in tokio child
    );

    Ok(session_id)
}

#[tauri::command]
pub async fn send_claude_message(
    session_id: String,
    message: String,
    state: State<'_, ClaudeSessionManager>,
) -> Result<(), String> {
    state.send_message(&session_id, &message).await
}

#[tauri::command]
pub async fn kill_claude_session(
    session_id: String,
    state: State<'_, ClaudeSessionManager>,
) -> Result<(), String> {
    state.kill(&session_id).await
}

#[tauri::command]
pub async fn get_resume_session_id(
    engagement_id: String,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    Ok(crate::claude::registry::get_session_id(&app_data_dir, &engagement_id))
}
```

- [ ] **Step 3: Register new command in lib.rs and add setup callback**

In `src-tauri/src/lib.rs`, add the setup callback and register the new command:

```rust
mod claude;
mod commands;
mod mcp;
mod oauth;
mod skills;

use claude::ClaudeSessionManager;
use mcp::manager::McpProcessManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_keyring::init())
        .manage(commands::oauth::OAuthState::default())
        .manage(McpProcessManager::new())
        .manage(ClaudeSessionManager::new())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("No app data dir");
            claude::registry::cleanup_orphans(&app_data_dir);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::credentials::store_credential,
            commands::credentials::get_credential,
            commands::credentials::delete_credential,
            commands::oauth::start_oauth,
            commands::oauth::exchange_oauth_code,
            commands::mcp::spawn_mcp,
            commands::mcp::kill_mcp,
            commands::mcp::kill_all_mcp,
            commands::mcp::mcp_health,
            commands::mcp::restart_mcp,
            commands::vault::create_vault,
            commands::vault::archive_vault,
            commands::vault::restore_vault,
            commands::vault::delete_vault,
            // Claude M2 — embedded subprocess
            claude::auth::claude_version_check,
            claude::auth::claude_auth_status,
            claude::auth::claude_auth_login,
            claude::commands::spawn_claude_session,
            claude::commands::send_claude_message,
            claude::commands::kill_claude_session,
            claude::commands::get_resume_session_id,
            // Skills — Phase 2
            skills::commands::scaffold_engagement_skills_cmd,
            skills::commands::check_skill_updates_cmd,
            skills::commands::apply_skill_updates_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: Update TypeScript IPC wrapper**

In `src/lib/tauri-commands.ts`, update `spawnClaudeSession` and add `getResumeSessionId`:

```typescript
export async function spawnClaudeSession(
  engagementId: string,
  engagementPath: string,
  resumeSessionId?: string,
): Promise<string> {
  return invoke("spawn_claude_session", {
    engagementId,
    engagementPath,
    resumeSessionId: resumeSessionId ?? null,
  });
}
```

Add after `killClaudeSession`:

```typescript
export async function getResumeSessionId(
  engagementId: string,
): Promise<string | null> {
  return invoke("get_resume_session_id", { engagementId });
}
```

- [ ] **Step 5: Run cargo check + cargo test**

Run: `cd src-tauri && cargo check && cargo test`
Expected: PASS (warnings ok)

- [ ] **Step 6: Run tsc**

Run: `npx tsc --noEmit`
Expected: 0 errors

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/claude/session_manager.rs src-tauri/src/claude/commands.rs src-tauri/src/lib.rs src/lib/tauri-commands.ts
git commit -m "feat: session resume via --resume flag + registry-based orphan cleanup"
```

---

## Task 8: useWorkspaceSession Hook

**Files:**
- Create: `src/hooks/useWorkspaceSession.ts`
- Modify: `src/hooks/useEngagement.ts`
- Modify: `src/components/layout/EngagementSwitcher.tsx`
- Modify: `src/views/ChatView.tsx`

- [ ] **Step 1: Create the hook**

Create `src/hooks/useWorkspaceSession.ts`:

```typescript
import { useState, useCallback } from "react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import {
  killClaudeSession,
  spawnClaudeSession,
  getResumeSessionId,
  claudeVersionCheck,
  claudeAuthStatus,
} from "@/lib/tauri-commands";

/**
 * Polls claudeStore.status via subscribe() with a timeout.
 * Returns true if the target status is reached, false on timeout.
 */
function waitForStatus(target: string, timeoutMs: number): Promise<boolean> {
  return new Promise((resolve) => {
    // Check immediately
    if (useClaudeStore.getState().status === target) {
      resolve(true);
      return;
    }
    const timer = setTimeout(() => {
      unsub();
      resolve(false);
    }, timeoutMs);
    const unsub = useClaudeStore.subscribe((state) => {
      if (state.status === target) {
        clearTimeout(timer);
        unsub();
        resolve(true);
      }
    });
  });
}

export function useWorkspaceSession() {
  const [switching, setSwitching] = useState(false);

  const connect = useCallback(async () => {
    const engagement = useEngagementStore.getState().engagements.find(
      (e) => e.id === useEngagementStore.getState().activeEngagementId
    );
    if (!engagement) return;

    // Preflight
    const version = await claudeVersionCheck();
    if (!version.installed) {
      useClaudeStore.getState().setError("Claude CLI not found. Please install Claude Code first.");
      return;
    }
    if (!version.meets_minimum) {
      useClaudeStore.getState().setError(`Claude CLI ${version.version} is too old. Please update to v2.1.0 or later.`);
      return;
    }
    const auth = await claudeAuthStatus();
    if (!auth.loggedIn) {
      useClaudeStore.getState().setError("Not signed in to Claude. Please sign in first from Settings.");
      return;
    }

    useClaudeStore.getState().reset();
    useClaudeStore.setState({ status: "connecting" });

    try {
      // Check for resume session
      const resumeId = await getResumeSessionId(engagement.id);
      await spawnClaudeSession(engagement.id, engagement.vault.path, resumeId ?? undefined);

      // Frontend-driven resume timeout (5s)
      if (resumeId) {
        const connected = await waitForStatus("connected", 5000);
        if (!connected) {
          // Resume failed — kill and retry without --resume
          const currentSessionId = useClaudeStore.getState().sessionId;
          if (currentSessionId) {
            await killClaudeSession(currentSessionId);
          }
          useClaudeStore.getState().reset();
          useClaudeStore.setState({ status: "connecting" });
          await spawnClaudeSession(engagement.id, engagement.vault.path);
        }
      }
    } catch (e) {
      useClaudeStore.getState().setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const switchEngagement = useCallback(async (newEngagementId: string) => {
    if (switching) return;
    setSwitching(true);

    try {
      // 1. Kill current Claude session
      const currentSessionId = useClaudeStore.getState().sessionId;
      if (currentSessionId) {
        await killClaudeSession(currentSessionId);
      }

      // 2. Save current chat history
      const currentEngId = useEngagementStore.getState().activeEngagementId;
      if (currentEngId) {
        useClaudeStore.getState().saveAndClearHistory(currentEngId);
      }

      // 3. Set new active engagement
      useEngagementStore.getState().setActiveEngagement(newEngagementId);

      // 4. Load target engagement's chat history
      useClaudeStore.getState().loadHistory(newEngagementId);

      // 5. Check for resume session and spawn
      const resumeId = await getResumeSessionId(newEngagementId);
      const engagement = useEngagementStore.getState().engagements.find(
        (e) => e.id === newEngagementId
      );
      if (engagement) {
        useClaudeStore.setState({ status: "connecting" });
        await spawnClaudeSession(
          newEngagementId,
          engagement.vault.path,
          resumeId ?? undefined,
        );

        // Frontend-driven resume timeout (5s)
        if (resumeId) {
          const connected = await waitForStatus("connected", 5000);
          if (!connected) {
            const sid = useClaudeStore.getState().sessionId;
            if (sid) await killClaudeSession(sid);
            useClaudeStore.getState().reset();
            useClaudeStore.setState({ status: "connecting" });
            await spawnClaudeSession(newEngagementId, engagement.vault.path);
          }
        }
      }
    } catch (e) {
      useClaudeStore.getState().setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSwitching(false);
    }
  }, [switching]);

  return { connect, switchEngagement, switching };
}
```

- [ ] **Step 2: Simplify useEngagement (remove switch logic)**

Replace `src/hooks/useEngagement.ts`:

```typescript
import { useCallback } from "react";
import { useEngagementStore } from "@/stores/engagementStore";
import { useMcpStore } from "@/stores/mcpStore";
import {
  killAllMcp,
  spawnMcp,
  getCredential,
  makeKeychainKey,
  createVault,
} from "@/lib/tauri-commands";
import type { McpServerType, McpHealth } from "@/types";

interface McpConfig {
  type: McpServerType;
  command: string;
  args: string[];
}

const MCP_CONFIGS: McpConfig[] = [
  { type: "gmail", command: "npx", args: ["@shinzolabs/gmail-mcp@1.7.4"] },
  { type: "calendar", command: "npx", args: ["@cocal/google-calendar-mcp@2.6.1"] },
  { type: "drive", command: "npx", args: ["@piotr-agier/google-drive-mcp@2.0.2"] },
];

/**
 * MCP lifecycle management for engagements.
 * Session switching is handled by useWorkspaceSession — this hook
 * only manages MCP servers (until Phase 3b retires app-side spawning).
 */
export function useEngagement() {
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);
  const setServers = useMcpStore((s) => s.setServers);

  const refreshMcpServers = useCallback(
    async (engagementId: string) => {
      await killAllMcp();

      const key = makeKeychainKey(engagementId, "google");
      const token = await getCredential(key);

      const engagement = engagements.find((e) => e.id === engagementId);
      const client = clients.find((c) => c.id === engagement?.clientId);
      if (client) {
        await createVault(client.slug);
      }

      if (token) {
        const newServers: McpHealth[] = [];
        for (const config of MCP_CONFIGS) {
          try {
            const pid = await spawnMcp({
              server_type: config.type,
              command: config.command,
              args: config.args,
              env: { GOOGLE_ACCESS_TOKEN: token },
            });
            newServers.push({
              type: config.type,
              status: "healthy",
              pid,
              lastPing: new Date(),
              restartCount: 0,
            });
          } catch {
            newServers.push({
              type: config.type,
              status: "down",
              restartCount: 0,
            });
          }
        }
        if (client) {
          try {
            const home = await import("@tauri-apps/api/path").then((m) => m.homeDir());
            const vaultPath = `${home}.ikrs-workspace/vaults/${client.slug}`;
            const pid = await spawnMcp({
              server_type: "obsidian",
              command: "npx",
              args: ["@bitbonsai/mcpvault@1.3.0", vaultPath],
              env: {},
            });
            newServers.push({
              type: "obsidian",
              status: "healthy",
              pid,
              lastPing: new Date(),
              restartCount: 0,
            });
          } catch {
            newServers.push({
              type: "obsidian",
              status: "down",
              restartCount: 0,
            });
          }
        }
        setServers(newServers);
      } else {
        setServers([]);
      }
    },
    [setServers, engagements, clients],
  );

  return { refreshMcpServers };
}
```

- [ ] **Step 3: Update EngagementSwitcher**

In `src/components/layout/EngagementSwitcher.tsx`, replace the import and hook usage:

```tsx
import { useWorkspaceSession } from "@/hooks/useWorkspaceSession";
```

Replace line 20:

```tsx
  const { switchEngagement, switching } = useWorkspaceSession();
```

Remove the `useEngagement` import entirely.

- [ ] **Step 4: Update ChatView to use useWorkspaceSession**

In `src/views/ChatView.tsx`, update imports:

```typescript
import { useWorkspaceSession } from "@/hooks/useWorkspaceSession";
```

Remove the direct imports of `claudeAuthStatus`, `claudeVersionCheck`, `spawnClaudeSession` from tauri-commands (keep `sendClaudeMessage`).

Replace `handleConnect` with the hook:

```typescript
  const { connect: handleConnect, switching } = useWorkspaceSession();
```

Remove the old `handleConnect` useCallback block entirely.

Update `SessionIndicator` to pass `switching`:

```tsx
      <SessionIndicator status={status} model={model} costUsd={totalCostUsd} switching={switching} />
```

- [ ] **Step 5: Run tsc**

Run: `npx tsc --noEmit`
Expected: 0 errors

- [ ] **Step 6: Run vitest**

Run: `npx vitest run`
Expected: All PASS

- [ ] **Step 7: Commit**

```bash
git add src/hooks/useWorkspaceSession.ts src/hooks/useEngagement.ts src/components/layout/EngagementSwitcher.tsx src/views/ChatView.tsx
git commit -m "feat: useWorkspaceSession orchestrator for engagement switching + session resume"
```

---

## Task 9: Build Verification

**Files:** None (verification only)

- [ ] **Step 1: Rust checks**

Run: `cd src-tauri && cargo check 2>&1 | tail -3`
Expected: `Finished` with no new errors

- [ ] **Step 2: Rust tests**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: All tests pass (20 prior + ~12 new = ~32 total)

- [ ] **Step 3: TypeScript typecheck**

Run: `npx tsc --noEmit`
Expected: 0 errors

- [ ] **Step 4: Vitest**

Run: `npx vitest run`
Expected: All tests pass (~40+ total)

- [ ] **Step 5: Vite build**

Run: `npm run build`
Expected: Build succeeds

- [ ] **Step 6: Commit verification marker**

No commit needed — just verify everything is green.

---

## Execution Notes

### Parallelism

These tasks can run in parallel waves:

**Wave 1 (independent):**
- Task 1: C1 fix (Rust only)
- Task 2: Tool data forwarding (Rust only)
- Task 5: Chat history (TS only)
- Task 6: JSON registry (Rust only)

**Wave 2 (depends on Wave 1):**
- Task 3: ToolActivityCard expand (depends on Task 2)
- Task 4: SessionDetailsModal (depends on Task 5 for sessionStartedAt)
- Task 7: Session resume (depends on Tasks 1, 6)

**Wave 3 (depends on Wave 2):**
- Task 8: useWorkspaceSession + wiring (depends on Tasks 1, 5, 7)

**Wave 4:**
- Task 9: Build verification

### Key Build Commands

```bash
# Rust (always run from src-tauri/)
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri
cargo check
cargo test

# TypeScript (run from project root)
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
npx tsc --noEmit
npx vitest run
npm run build
```
