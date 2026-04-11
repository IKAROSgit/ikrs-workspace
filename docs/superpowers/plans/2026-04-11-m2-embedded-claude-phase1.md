# M2 Embedded Claude — Phase 1: Core Subprocess Implementation Plan

> **STATUS: COMPLETE** — All 13 tasks executed 2026-04-11. Codex review: 8/10 APPROVED WITH CONDITIONS (C1+C2 fixed, C3 deferred to integration testing). Commit: `0bc4d1b`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the M1 external-terminal Claude integration with an embedded headless subprocess piped through the Rust backend into a React chat UI.

**Architecture:** Claude Code CLI v2.1.92 spawned as a long-lived child process by the Rust backend using `--print --input-format stream-json --output-format stream-json --verbose --disallowed-tools Bash`. Stdout is parsed line-by-line into typed Tauri events. React subscribes to these events and renders a curated assistant experience (streaming text + tool status cards). Auth via `claude auth status` / `claude auth login` (OAuth, no API keys).

**Tech Stack:** Rust (tokio, serde_json), Tauri 2.x IPC events, React 19, TypeScript 5, Zustand v5

**Spec:** `docs/specs/embedded-claude-architecture.md` (992 lines, Codex-approved)

---

## File Structure

### Files to CREATE

| File | Responsibility |
|------|---------------|
| `src-tauri/src/claude/mod.rs` | Module root — re-exports session manager, stream parser, types |
| `src-tauri/src/claude/types.rs` | Rust types for stream events, session state, auth status |
| `src-tauri/src/claude/session_manager.rs` | `ClaudeSessionManager` — spawn, send, kill, orphan cleanup |
| `src-tauri/src/claude/stream_parser.rs` | Parse NDJSON stdout → typed Tauri events, hook filtering |
| `src-tauri/src/claude/auth.rs` | `claude_auth_status()`, `claude_auth_login()` commands |
| `src-tauri/src/claude/commands.rs` | Tauri commands: `spawn_claude_session`, `send_claude_message`, `kill_claude_session` |
| `src/types/claude.ts` | TypeScript types for chat messages, tool activity, session state |
| `src/stores/claudeStore.ts` | Zustand store for Claude session state, messages, tools |
| `src/hooks/useClaudeStream.ts` | Subscribe to Tauri events, dispatch to store |
| `src/views/ChatView.tsx` | Full chat UI — message list, tool cards, input bar, session indicator |
| `src/components/chat/MessageBubble.tsx` | Single chat message (streaming text) |
| `src/components/chat/ToolActivityCard.tsx` | Tool status card (spinner → checkmark) |
| `src/components/chat/InputBar.tsx` | Text input with send button |
| `src/components/chat/SessionIndicator.tsx` | Connection status dot + engagement name |
| `tests/unit/stores/claudeStore.test.ts` | Store action tests |
| `tests/unit/claude-types.test.ts` | Type guard tests for stream events |

### Files to MODIFY

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Register `ClaudeSessionManager` state, replace old commands with new ones |
| `src-tauri/src/commands/mod.rs` | Remove `pub mod claude;` (moved to dedicated module) |
| `src/types/index.ts` | Remove old `ClaudeSession` type, re-export from `claude.ts` |
| `src/lib/tauri-commands.ts` | Replace old claude commands with new ones |
| `src/Router.tsx` | Change lazy import from `ClaudeView` to `ChatView` |

### Files to DELETE

| File | Reason |
|------|--------|
| `src-tauri/src/commands/claude.rs` | Replaced by `src-tauri/src/claude/` module |
| `src/views/ClaudeView.tsx` | Replaced by `ChatView.tsx` |
| `src/hooks/useClaude.ts` | Replaced by `claudeStore.ts` + `useClaudeStream.ts` |

---

## Task 1: Rust Stream Event Types

**Files:**
- Create: `src-tauri/src/claude/types.rs`
- Create: `src-tauri/src/claude/mod.rs`

- [x] **Step 1: Create the claude module directory**

```bash
mkdir -p src-tauri/src/claude
```

- [x] **Step 2: Write `src-tauri/src/claude/types.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Raw stream-json event from Claude CLI stdout.
/// Every line is one of these. The parser must handle unknown variants gracefully.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "system")]
    System(SystemEvent),
    #[serde(rename = "assistant")]
    Assistant(AssistantEvent),
    #[serde(rename = "user")]
    User(UserEvent),
    #[serde(rename = "rate_limit_event")]
    RateLimit(serde_json::Value),
    #[serde(rename = "result")]
    Result(ResultEvent),
}

#[derive(Debug, Deserialize)]
pub struct SystemEvent {
    pub subtype: String,
    pub session_id: Option<String>,
    /// Present on init events
    pub tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub claude_code_version: Option<String>,
    /// Present on hook events
    pub hook_id: Option<String>,
    pub hook_name: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AssistantEvent {
    pub message: AssistantMessage,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub usage: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct UserEvent {
    pub message: Option<UserMessage>,
    pub tool_use_result: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserMessage {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
pub struct ResultEvent {
    pub subtype: String,
    #[serde(default)]
    pub is_error: bool,
    pub result: Option<String>,
    pub session_id: Option<String>,
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub num_turns: Option<u32>,
    pub stop_reason: Option<String>,
}

/// Typed Tauri event payloads emitted to the frontend
#[derive(Debug, Clone, Serialize)]
pub struct SessionReadyPayload {
    pub session_id: String,
    pub tools: Vec<String>,
    pub model: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextDeltaPayload {
    pub text: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStartPayload {
    pub tool_id: String,
    pub tool_name: String,
    pub friendly_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolEndPayload {
    pub tool_id: String,
    pub success: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnCompletePayload {
    pub session_id: String,
    pub cost_usd: f64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEndPayload {
    pub session_id: String,
    pub exit_code: Option<i32>,
    pub reason: String,
}

/// Auth status returned by `claude auth status`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    #[serde(rename = "loggedIn")]
    pub logged_in: bool,
    #[serde(rename = "authMethod")]
    pub auth_method: Option<String>,
    #[serde(rename = "apiProvider")]
    pub api_provider: Option<String>,
}

/// CLI version check
#[derive(Debug, Clone, Serialize)]
pub struct VersionCheck {
    pub installed: bool,
    pub version: Option<String>,
    pub meets_minimum: bool,
}

pub const MIN_CLAUDE_VERSION: &str = "2.1.0";
```

- [x] **Step 3: Write `src-tauri/src/claude/mod.rs`**

```rust
pub mod auth;
pub mod commands;
pub mod session_manager;
pub mod stream_parser;
pub mod types;

pub use session_manager::ClaudeSessionManager;
pub use types::*;
```

- [x] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: May warn about unused modules (auth, commands, etc. don't exist yet) — that's fine. No type errors.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/claude/
git commit -m "feat(claude): add stream event types and module structure for M2 embedded Claude"
```

---

## Task 2: Stream Parser

**Files:**
- Create: `src-tauri/src/claude/stream_parser.rs`

- [x] **Step 1: Write `src-tauri/src/claude/stream_parser.rs`**

```rust
use crate::claude::types::*;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;

/// Generates a user-friendly label for tool_use events.
fn friendly_label(tool_name: &str, input: &serde_json::Value) -> String {
    let short = |v: Option<&str>| -> String {
        v.map(|p| {
            p.rsplit('/').next().unwrap_or(p).to_string()
        })
        .unwrap_or_else(|| "...".to_string())
    };

    match tool_name {
        "Write" => format!("Writing {}", short(input["file_path"].as_str())),
        "Edit" => format!("Editing {}", short(input["file_path"].as_str())),
        "Read" => format!("Reading {}", short(input["file_path"].as_str())),
        "Glob" => format!(
            "Searching files matching {}",
            input["pattern"].as_str().unwrap_or("...")
        ),
        "Grep" => format!(
            "Searching for \"{}\"",
            input["pattern"].as_str().unwrap_or("...")
        ),
        "WebSearch" => format!(
            "Searching the web for \"{}\"",
            input["query"].as_str().unwrap_or("...")
        ),
        "WebFetch" => format!(
            "Fetching {}",
            input["url"].as_str().unwrap_or("...")
        ),
        _ => "Working...".to_string(),
    }
}

/// Truncates tool result content for the summary payload.
fn summarize_tool_result(content: &Option<serde_json::Value>, is_error: bool) -> String {
    if is_error {
        return "Error".to_string();
    }
    match content {
        Some(serde_json::Value::String(s)) => {
            if s.len() > 80 {
                format!("{}...", &s[..77])
            } else {
                s.clone()
            }
        }
        Some(_) => "Completed".to_string(),
        None => "Completed".to_string(),
    }
}

/// Counter for generating unique message IDs within a session.
struct MessageIdGen {
    counter: u64,
}

impl MessageIdGen {
    fn new() -> Self {
        Self { counter: 0 }
    }
    fn next(&mut self) -> String {
        self.counter += 1;
        format!("msg_{}", self.counter)
    }
}

/// Reads Claude CLI stdout line-by-line and emits typed Tauri events.
/// Returns when the stream ends (process exited or pipe broken).
pub async fn parse_stream(stdout: ChildStdout, app: AppHandle) {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut msg_id_gen = MessageIdGen::new();
    let mut current_msg_id = msg_id_gen.next();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                handle_line(&line, &app, &mut msg_id_gen, &mut current_msg_id);
            }
            Ok(None) => {
                // EOF — process exited or pipe closed
                break;
            }
            Err(e) => {
                log::error!("Stream read error: {e}");
                let _ = app.emit(
                    "claude:error",
                    ErrorPayload {
                        message: format!("Stream read error: {e}"),
                    },
                );
                break;
            }
        }
    }
}

fn handle_line(
    line: &str,
    app: &AppHandle,
    msg_id_gen: &mut MessageIdGen,
    current_msg_id: &mut String,
) {
    // First try to determine the type field without full deserialization
    let raw: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            log::debug!("Non-JSON line from Claude CLI: {e}");
            return;
        }
    };

    let event_type = raw["type"].as_str().unwrap_or("");

    match event_type {
        "system" => handle_system_event(&raw, app),
        "assistant" => handle_assistant_event(&raw, app, msg_id_gen, current_msg_id),
        "user" => handle_user_event(&raw, app),
        "result" => handle_result_event(&raw, app),
        "rate_limit_event" => { /* silently drop — internal bookkeeping */ }
        _ => {
            log::debug!("Unknown stream event type: {}", event_type);
        }
    }
}

fn handle_system_event(raw: &serde_json::Value, app: &AppHandle) {
    let subtype = raw["subtype"].as_str().unwrap_or("");
    match subtype {
        "hook_started" | "hook_response" => {
            // Silently filtered — consultant doesn't need to see hook lifecycle
            log::debug!("Filtered hook event: {}", subtype);
        }
        "init" => {
            let payload = SessionReadyPayload {
                session_id: raw["session_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                tools: raw["tools"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                model: raw["model"].as_str().unwrap_or("unknown").to_string(),
                cwd: raw["cwd"].as_str().unwrap_or("").to_string(),
            };
            let _ = app.emit("claude:session-ready", payload);
        }
        _ => {
            log::debug!("Unknown system subtype: {}", subtype);
        }
    }
}

fn handle_assistant_event(
    raw: &serde_json::Value,
    app: &AppHandle,
    msg_id_gen: &mut MessageIdGen,
    current_msg_id: &mut String,
) {
    let content = match raw["message"]["content"].as_array() {
        Some(arr) => arr,
        None => return,
    };

    for block in content {
        let block_type = block["type"].as_str().unwrap_or("");
        match block_type {
            "text" => {
                if let Some(text) = block["text"].as_str() {
                    let _ = app.emit(
                        "claude:text-delta",
                        TextDeltaPayload {
                            text: text.to_string(),
                            message_id: current_msg_id.clone(),
                        },
                    );
                }
            }
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
                    },
                );
            }
            "thinking" => {
                // Filtered — internal reasoning not shown to consultant
            }
            _ => {
                log::debug!("Unknown content block type: {}", block_type);
            }
        }
    }

    // After processing a full assistant message with text, advance the message ID
    // so the next text block starts a new bubble
    let has_text = content
        .iter()
        .any(|b| b["type"].as_str() == Some("text"));
    if has_text {
        *current_msg_id = msg_id_gen.next();
    }
}

fn handle_user_event(raw: &serde_json::Value, app: &AppHandle) {
    // User events contain tool_result content blocks
    let content = match raw["message"]["content"].as_array() {
        Some(arr) => arr,
        None => return,
    };

    for block in content {
        if block["type"].as_str() == Some("tool_result") {
            let tool_id = block["tool_use_id"].as_str().unwrap_or("unknown");
            let is_error = block["is_error"].as_bool().unwrap_or(false);
            let _ = app.emit(
                "claude:tool-end",
                ToolEndPayload {
                    tool_id: tool_id.to_string(),
                    success: !is_error,
                    summary: summarize_tool_result(&block.get("content").cloned(), is_error),
                },
            );
        }
    }
}

fn handle_result_event(raw: &serde_json::Value, app: &AppHandle) {
    let subtype = raw["subtype"].as_str().unwrap_or("");
    let is_error = raw["is_error"].as_bool().unwrap_or(false);

    if is_error || subtype == "error" {
        let _ = app.emit(
            "claude:error",
            ErrorPayload {
                message: raw["result"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },
        );
    } else {
        let _ = app.emit(
            "claude:turn-complete",
            TurnCompletePayload {
                session_id: raw["session_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                cost_usd: raw["total_cost_usd"].as_f64().unwrap_or(0.0),
                duration_ms: raw["duration_ms"].as_u64().unwrap_or(0),
            },
        );
    }
}
```

- [x] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: Compiles (may warn about unused imports for modules not yet created — that's OK at this step).

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/claude/stream_parser.rs
git commit -m "feat(claude): add stream parser with hook filtering and friendly tool labels"
```

---

## Task 3: Claude Session Manager

**Files:**
- Create: `src-tauri/src/claude/session_manager.rs`

- [x] **Step 1: Write `src-tauri/src/claude/session_manager.rs`**

```rust
use crate::claude::stream_parser::parse_stream;
use crate::claude::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

struct ClaudeSession {
    stdin: tokio::process::ChildStdin,
    session_id: String,
    engagement_id: String,
}

pub struct ClaudeSessionManager {
    sessions: Arc<Mutex<HashMap<String, ClaudeSession>>>,
    max_sessions: usize,
}

impl ClaudeSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            max_sessions: 1, // One session at a time for M2
        }
    }

    /// Spawn a new Claude CLI subprocess for an engagement.
    /// Returns the session_id.
    pub async fn spawn(
        &self,
        engagement_id: String,
        engagement_path: String,
        app: AppHandle,
    ) -> Result<String, String> {
        // Enforce max sessions — kill existing if at limit
        {
            let mut sessions = self.sessions.lock().await;
            if sessions.len() >= self.max_sessions {
                let keys: Vec<String> = sessions.keys().cloned().collect();
                for key in keys {
                    if let Some(session) = sessions.remove(&key) {
                        drop(session.stdin); // Close stdin to signal EOF
                        let _ = app.emit(
                            "claude:session-ended",
                            SessionEndPayload {
                                session_id: key.clone(),
                                exit_code: Some(0),
                                reason: "Replaced by new session".to_string(),
                            },
                        );
                    }
                }
            }
        }

        let session_id = uuid::Uuid::new_v4().to_string();

        let mut child = Command::new("claude")
            .args([
                "--print",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--disallowed-tools",
                "Bash",
            ])
            .current_dir(&engagement_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn claude: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture Claude stdout")?;
        let stdin = child
            .stdin
            .take()
            .ok_or("Failed to capture Claude stdin")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Failed to capture Claude stderr")?;

        // Spawn stream parser task (reads stdout → emits Tauri events)
        let parser_app = app.clone();
        tokio::spawn(async move {
            parse_stream(stdout, parser_app).await;
        });

        // Spawn stderr reader (logs to debug, emits claude:stderr)
        let stderr_app = app.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
            while let Ok(Some(line)) = lines.next_line().await {
                log::debug!("Claude stderr: {}", line);
                let _ = stderr_app.emit(
                    "claude:stderr",
                    serde_json::json!({ "line": line }),
                );
            }
        });

        // Spawn process monitor task (detects crashes)
        let monitor_app = app.clone();
        let monitor_session_id = session_id.clone();
        tokio::spawn(async move {
            monitor_process(child, monitor_session_id, monitor_app).await;
        });

        // Store session
        let session = ClaudeSession {
            stdin,
            session_id: session_id.clone(),
            engagement_id,
        };
        self.sessions.lock().await.insert(session_id.clone(), session);

        Ok(session_id)
    }

    /// Send a user message to an active session.
    pub async fn send_message(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;

        let msg = serde_json::json!({
            "type": "user",
            "content": [{ "type": "text", "text": text }]
        });
        let mut payload = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        payload.push('\n');

        session
            .stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to Claude stdin: {e}"))?;
        session
            .stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush Claude stdin: {e}"))?;

        Ok(())
    }

    /// Kill a session by closing its stdin (triggers graceful exit).
    pub async fn kill(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        sessions
            .remove(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;
        // Dropping the session closes stdin, which signals EOF to the CLI process.
        // The monitor task will detect the exit and emit claude:session-ended.
        Ok(())
    }

    /// Check if a session is active.
    pub async fn has_session(&self) -> bool {
        !self.sessions.lock().await.is_empty()
    }
}

/// Monitors a Claude child process and emits events on exit.
async fn monitor_process(mut child: Child, session_id: String, app: AppHandle) {
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

fn classify_exit(code: Option<i32>) -> String {
    match code {
        Some(0) => "Session ended normally".into(),
        Some(1) => "Claude CLI error".into(),
        Some(137) => "Process killed (OOM or SIGKILL)".into(),
        Some(143) => "Process terminated (SIGTERM)".into(),
        None => "Process terminated by signal".into(),
        Some(c) => format!("Unexpected exit code: {c}"),
    }
}
```

- [x] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: Compiles. Warnings about unused fields are OK.

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/claude/session_manager.rs
git commit -m "feat(claude): add session manager with spawn, send, kill, and crash monitoring"
```

---

## Task 4: Auth Commands

**Files:**
- Create: `src-tauri/src/claude/auth.rs`

- [x] **Step 1: Write `src-tauri/src/claude/auth.rs`**

```rust
use crate::claude::types::{AuthStatus, VersionCheck, MIN_CLAUDE_VERSION};
use std::process::Command;

/// Check if Claude CLI is installed and meets minimum version.
#[tauri::command]
pub async fn claude_version_check() -> Result<VersionCheck, String> {
    let output = Command::new("claude")
        .arg("--version")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Version string is like "2.1.92 (Claude Code)" — extract semver
            let semver = version.split_whitespace().next().unwrap_or("").to_string();
            let meets_minimum = compare_versions(&semver, MIN_CLAUDE_VERSION);
            Ok(VersionCheck {
                installed: true,
                version: Some(semver),
                meets_minimum,
            })
        }
        _ => Ok(VersionCheck {
            installed: false,
            version: None,
            meets_minimum: false,
        }),
    }
}

/// Check Claude CLI authentication status.
#[tauri::command]
pub async fn claude_auth_status() -> Result<AuthStatus, String> {
    let output = Command::new("claude")
        .args(["auth", "status"])
        .output()
        .map_err(|e| format!("Failed to check claude auth: {e}"))?;

    if !output.status.success() {
        return Ok(AuthStatus {
            logged_in: false,
            auth_method: None,
            api_provider: None,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|e| {
        format!("Failed to parse claude auth status: {e}")
    })
}

/// Initiate Claude CLI login (opens system browser for OAuth).
#[tauri::command]
pub async fn claude_auth_login() -> Result<(), String> {
    let status = Command::new("claude")
        .args(["auth", "login"])
        .status()
        .map_err(|e| format!("Failed to start claude auth login: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Claude auth login failed".to_string())
    }
}

/// Simple semver comparison: returns true if `version` >= `minimum`.
fn compare_versions(version: &str, minimum: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let v = parse(version);
    let m = parse(minimum);

    for i in 0..3 {
        let a = v.get(i).copied().unwrap_or(0);
        let b = m.get(i).copied().unwrap_or(0);
        if a > b {
            return true;
        }
        if a < b {
            return false;
        }
    }
    true // Equal
}
```

- [x] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: Clean compile.

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/claude/auth.rs
git commit -m "feat(claude): add auth status check and login commands"
```

---

## Task 5: Tauri Commands (IPC Bridge)

**Files:**
- Create: `src-tauri/src/claude/commands.rs`

- [x] **Step 1: Write `src-tauri/src/claude/commands.rs`**

```rust
use crate::claude::session_manager::ClaudeSessionManager;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String> {
    state.spawn(engagement_id, engagement_path, app).await
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
```

- [x] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: Clean compile.

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/claude/commands.rs
git commit -m "feat(claude): add Tauri IPC command wrappers for session management"
```

---

## Task 6: Wire Into lib.rs (Remove Old, Add New)

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Delete: `src-tauri/src/commands/claude.rs`

- [x] **Step 1: Delete old `commands/claude.rs`**

```bash
rm src-tauri/src/commands/claude.rs
```

- [x] **Step 2: Update `src-tauri/src/commands/mod.rs`**

Remove `pub mod claude;` line. The file should become:

```rust
pub mod credentials;
pub mod mcp;
pub mod oauth;
pub mod vault;
```

- [x] **Step 3: Update `src-tauri/src/lib.rs`**

Replace the entire file with:

```rust
mod claude;
mod commands;
mod mcp;
mod oauth;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [x] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -30`
Expected: Clean compile. No references to old `commands::claude::*`.

- [x] **Step 5: Commit**

```bash
git add -A src-tauri/src/
git commit -m "feat(claude): wire M2 session manager into Tauri app, remove old M1 claude commands"
```

---

## Task 7: TypeScript Types

**Files:**
- Create: `src/types/claude.ts`
- Modify: `src/types/index.ts`

- [x] **Step 1: Write `src/types/claude.ts`**

```typescript
export type ClaudeSessionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "thinking"
  | "error";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  timestamp: Date;
  isStreaming: boolean;
}

export interface ToolActivity {
  toolId: string;
  toolName: string;
  friendlyLabel: string;
  status: "running" | "success" | "error";
  summary?: string;
  startedAt: Date;
  completedAt?: Date;
}

export interface SessionReadyPayload {
  session_id: string;
  tools: string[];
  model: string;
  cwd: string;
}

export interface TextDeltaPayload {
  text: string;
  message_id: string;
}

export interface ToolStartPayload {
  tool_id: string;
  tool_name: string;
  friendly_label: string;
}

export interface ToolEndPayload {
  tool_id: string;
  success: boolean;
  summary: string;
}

export interface TurnCompletePayload {
  session_id: string;
  cost_usd: number;
  duration_ms: number;
}

export interface ErrorPayload {
  message: string;
}

export interface SessionEndPayload {
  session_id: string;
  exit_code: number | null;
  reason: string;
}

export interface AuthStatus {
  loggedIn: boolean;
  authMethod: string | null;
  apiProvider: string | null;
}

export interface VersionCheck {
  installed: boolean;
  version: string | null;
  meets_minimum: boolean;
}
```

- [x] **Step 2: Update `src/types/index.ts`**

Remove the old `ClaudeSession` interface and add the re-export. Replace:

```typescript
export interface ClaudeSession {
  engagementId: string;
  pid: number;
  startedAt: Date;
  projectPath: string;
}
```

With:

```typescript
export type { ChatMessage, ToolActivity, ClaudeSessionStatus, AuthStatus, VersionCheck } from "./claude";
```

- [x] **Step 3: Verify types compile**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | head -20`
Expected: May have pre-existing errors from other views but no new errors from `claude.ts`.

- [x] **Step 4: Commit**

```bash
git add src/types/
git commit -m "feat(claude): add TypeScript types for M2 chat messages, tool activity, and session state"
```

---

## Task 8: Zustand Claude Store

**Files:**
- Create: `src/stores/claudeStore.ts`
- Create: `tests/unit/stores/claudeStore.test.ts`

- [x] **Step 1: Write the test file `tests/unit/stores/claudeStore.test.ts`**

```typescript
import { describe, it, expect, beforeEach } from "vitest";
import { useClaudeStore } from "@/stores/claudeStore";

describe("claudeStore", () => {
  beforeEach(() => {
    useClaudeStore.getState().reset();
  });

  it("starts disconnected with empty messages", () => {
    const state = useClaudeStore.getState();
    expect(state.status).toBe("disconnected");
    expect(state.messages).toEqual([]);
    expect(state.sessionId).toBeNull();
  });

  it("setSessionReady transitions to connected", () => {
    useClaudeStore.getState().setSessionReady("sess-1", ["Read", "Write"], "claude-sonnet-4-6");
    const state = useClaudeStore.getState();
    expect(state.status).toBe("connected");
    expect(state.sessionId).toBe("sess-1");
  });

  it("addUserMessage appends a user message", () => {
    useClaudeStore.getState().addUserMessage("Hello Claude");
    const state = useClaudeStore.getState();
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].role).toBe("user");
    expect(state.messages[0].text).toBe("Hello Claude");
    expect(state.status).toBe("thinking");
  });

  it("addTextDelta creates or appends to assistant message", () => {
    useClaudeStore.getState().addTextDelta("msg_1", "Hello");
    useClaudeStore.getState().addTextDelta("msg_1", " world");
    const state = useClaudeStore.getState();
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].role).toBe("assistant");
    expect(state.messages[0].text).toBe("Hello world");
    expect(state.messages[0].isStreaming).toBe(true);
  });

  it("startTool adds to activeTools", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading proposal.md");
    const state = useClaudeStore.getState();
    expect(state.activeTools).toHaveLength(1);
    expect(state.activeTools[0].toolId).toBe("tu_1");
    expect(state.activeTools[0].status).toBe("running");
  });

  it("endTool updates tool status", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading proposal.md");
    useClaudeStore.getState().endTool("tu_1", true, "Completed");
    const state = useClaudeStore.getState();
    expect(state.activeTools[0].status).toBe("success");
  });

  it("completeTurn transitions back to connected", () => {
    useClaudeStore.getState().setSessionReady("sess-1", [], "model");
    useClaudeStore.getState().addUserMessage("test");
    useClaudeStore.getState().completeTurn(0.05, 1500);
    const state = useClaudeStore.getState();
    expect(state.status).toBe("connected");
    expect(state.totalCostUsd).toBe(0.05);
  });

  it("setError transitions to error status", () => {
    useClaudeStore.getState().setError("Network failed");
    const state = useClaudeStore.getState();
    expect(state.status).toBe("error");
    expect(state.error).toBe("Network failed");
  });

  it("reset clears everything", () => {
    useClaudeStore.getState().setSessionReady("sess-1", [], "model");
    useClaudeStore.getState().addUserMessage("test");
    useClaudeStore.getState().reset();
    const state = useClaudeStore.getState();
    expect(state.status).toBe("disconnected");
    expect(state.messages).toEqual([]);
    expect(state.sessionId).toBeNull();
  });
});
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/stores/claudeStore.test.ts 2>&1 | tail -20`
Expected: FAIL — module `@/stores/claudeStore` not found.

- [x] **Step 3: Write `src/stores/claudeStore.ts`**

```typescript
import { create } from "zustand";
import type { ChatMessage, ToolActivity, ClaudeSessionStatus } from "@/types/claude";

interface ClaudeState {
  sessionId: string | null;
  status: ClaudeSessionStatus;
  messages: ChatMessage[];
  activeTools: ToolActivity[];
  totalCostUsd: number;
  error: string | null;
  availableTools: string[];
  model: string | null;

  setSessionReady: (sessionId: string, tools: string[], model: string) => void;
  addUserMessage: (text: string) => void;
  addTextDelta: (messageId: string, text: string) => void;
  startTool: (toolId: string, toolName: string, friendlyLabel: string) => void;
  endTool: (toolId: string, success: boolean, summary: string) => void;
  completeTurn: (costUsd: number, durationMs: number) => void;
  setError: (message: string) => void;
  setDisconnected: (reason: string) => void;
  reset: () => void;
}

const initialState = {
  sessionId: null as string | null,
  status: "disconnected" as ClaudeSessionStatus,
  messages: [] as ChatMessage[],
  activeTools: [] as ToolActivity[],
  totalCostUsd: 0,
  error: null as string | null,
  availableTools: [] as string[],
  model: null as string | null,
};

export const useClaudeStore = create<ClaudeState>()((set) => ({
  ...initialState,

  setSessionReady: (sessionId, tools, model) =>
    set({
      sessionId,
      status: "connected",
      availableTools: tools,
      model,
      error: null,
    }),

  addUserMessage: (text) =>
    set((state) => ({
      messages: [
        ...state.messages,
        {
          id: `user_${Date.now()}`,
          role: "user" as const,
          text,
          timestamp: new Date(),
          isStreaming: false,
        },
      ],
      status: "thinking",
    })),

  addTextDelta: (messageId, text) =>
    set((state) => {
      const existing = state.messages.find(
        (m) => m.id === messageId && m.role === "assistant"
      );
      if (existing) {
        return {
          messages: state.messages.map((m) =>
            m.id === messageId
              ? { ...m, text: m.text + text, isStreaming: true }
              : m
          ),
        };
      }
      return {
        messages: [
          ...state.messages,
          {
            id: messageId,
            role: "assistant" as const,
            text,
            timestamp: new Date(),
            isStreaming: true,
          },
        ],
      };
    }),

  startTool: (toolId, toolName, friendlyLabel) =>
    set((state) => ({
      activeTools: [
        ...state.activeTools,
        {
          toolId,
          toolName,
          friendlyLabel,
          status: "running" as const,
          startedAt: new Date(),
        },
      ],
    })),

  endTool: (toolId, success, summary) =>
    set((state) => ({
      activeTools: state.activeTools.map((t) =>
        t.toolId === toolId
          ? {
              ...t,
              status: (success ? "success" : "error") as "success" | "error",
              summary,
              completedAt: new Date(),
            }
          : t
      ),
    })),

  completeTurn: (costUsd, _durationMs) =>
    set((state) => ({
      status: state.sessionId ? "connected" : "disconnected",
      totalCostUsd: state.totalCostUsd + costUsd,
      messages: state.messages.map((m) =>
        m.isStreaming ? { ...m, isStreaming: false } : m
      ),
    })),

  setError: (message) =>
    set({
      status: "error",
      error: message,
    }),

  setDisconnected: (reason) =>
    set({
      status: "disconnected",
      sessionId: null,
      error: reason || null,
    }),

  reset: () => set(initialState),
}));
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/stores/claudeStore.test.ts 2>&1 | tail -20`
Expected: All 8 tests PASS.

- [x] **Step 5: Commit**

```bash
git add src/stores/claudeStore.ts tests/unit/stores/claudeStore.test.ts
git commit -m "feat(claude): add Zustand claudeStore with TDD (8 tests passing)"
```

---

## Task 9: Tauri Event Hook

**Files:**
- Create: `src/hooks/useClaudeStream.ts`

- [x] **Step 1: Write `src/hooks/useClaudeStream.ts`**

```typescript
import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useClaudeStore } from "@/stores/claudeStore";
import type {
  SessionReadyPayload,
  TextDeltaPayload,
  ToolStartPayload,
  ToolEndPayload,
  TurnCompletePayload,
  ErrorPayload,
  SessionEndPayload,
} from "@/types/claude";

/**
 * Subscribe to all Claude Tauri events and dispatch to the store.
 * Call this once at the ChatView level.
 */
export function useClaudeStream(): void {
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      const store = useClaudeStore.getState;

      unlisteners.push(
        await listen<SessionReadyPayload>("claude:session-ready", (event) => {
          store().setSessionReady(
            event.payload.session_id,
            event.payload.tools,
            event.payload.model
          );
        })
      );

      unlisteners.push(
        await listen<TextDeltaPayload>("claude:text-delta", (event) => {
          store().addTextDelta(
            event.payload.message_id,
            event.payload.text
          );
        })
      );

      unlisteners.push(
        await listen<ToolStartPayload>("claude:tool-start", (event) => {
          store().startTool(
            event.payload.tool_id,
            event.payload.tool_name,
            event.payload.friendly_label
          );
        })
      );

      unlisteners.push(
        await listen<ToolEndPayload>("claude:tool-end", (event) => {
          store().endTool(
            event.payload.tool_id,
            event.payload.success,
            event.payload.summary
          );
        })
      );

      unlisteners.push(
        await listen<TurnCompletePayload>("claude:turn-complete", (event) => {
          store().completeTurn(
            event.payload.cost_usd,
            event.payload.duration_ms
          );
        })
      );

      unlisteners.push(
        await listen<ErrorPayload>("claude:error", (event) => {
          store().setError(event.payload.message);
        })
      );

      unlisteners.push(
        await listen<SessionEndPayload>("claude:session-ended", (event) => {
          store().setDisconnected(event.payload.reason);
        })
      );

      unlisteners.push(
        await listen<SessionEndPayload>("claude:session-crashed", (event) => {
          store().setError(
            `Session crashed: ${event.payload.reason}`
          );
        })
      );
    };

    setup();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);
}
```

- [x] **Step 2: Commit**

```bash
git add src/hooks/useClaudeStream.ts
git commit -m "feat(claude): add useClaudeStream hook to bridge Tauri events to Zustand store"
```

---

## Task 10: Tauri Command Bindings (TypeScript)

**Files:**
- Modify: `src/lib/tauri-commands.ts`

- [x] **Step 1: Replace the old Claude command bindings**

In `src/lib/tauri-commands.ts`, remove the old Claude section (lines 104-122) and replace with:

```typescript
// Claude M2 — Embedded Subprocess
import type { AuthStatus, VersionCheck } from "@/types/claude";

export async function claudeVersionCheck(): Promise<VersionCheck> {
  return invoke("claude_version_check");
}

export async function claudeAuthStatus(): Promise<AuthStatus> {
  return invoke("claude_auth_status");
}

export async function claudeAuthLogin(): Promise<void> {
  return invoke("claude_auth_login");
}

export async function spawnClaudeSession(
  engagementId: string,
  engagementPath: string,
): Promise<string> {
  return invoke("spawn_claude_session", { engagementId, engagementPath });
}

export async function sendClaudeMessage(
  sessionId: string,
  message: string,
): Promise<void> {
  return invoke("send_claude_message", { sessionId, message });
}

export async function killClaudeSession(sessionId: string): Promise<void> {
  return invoke("kill_claude_session", { sessionId });
}
```

- [x] **Step 2: Verify TypeScript compiles**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | head -20`
Expected: No new errors from tauri-commands.ts.

- [x] **Step 3: Commit**

```bash
git add src/lib/tauri-commands.ts
git commit -m "feat(claude): update TypeScript command bindings for M2 embedded session API"
```

---

## Task 11: Chat UI Components

**Files:**
- Create: `src/components/chat/MessageBubble.tsx`
- Create: `src/components/chat/ToolActivityCard.tsx`
- Create: `src/components/chat/InputBar.tsx`
- Create: `src/components/chat/SessionIndicator.tsx`

- [x] **Step 1: Create directory**

```bash
mkdir -p src/components/chat
```

- [x] **Step 2: Write `src/components/chat/MessageBubble.tsx`**

```tsx
import { cn } from "@/lib/utils";
import type { ChatMessage } from "@/types/claude";

interface MessageBubbleProps {
  message: ChatMessage;
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === "user";

  return (
    <div className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[80%] rounded-lg px-4 py-2 text-sm whitespace-pre-wrap",
          isUser
            ? "bg-primary text-primary-foreground"
            : "bg-muted text-foreground"
        )}
      >
        {message.text}
        {message.isStreaming && (
          <span className="inline-block w-1.5 h-4 ml-0.5 bg-current animate-pulse" />
        )}
      </div>
    </div>
  );
}
```

- [x] **Step 3: Write `src/components/chat/ToolActivityCard.tsx`**

```tsx
import { cn } from "@/lib/utils";
import { Loader2, CheckCircle, XCircle } from "lucide-react";
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
  const icon = TOOL_ICONS[tool.toolName] ?? "\u{2699}\u{FE0F}";

  return (
    <div
      className={cn(
        "flex items-center gap-2 px-3 py-1.5 rounded-md text-xs",
        "bg-muted/50 border border-border/50"
      )}
    >
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
  );
}
```

- [x] **Step 4: Write `src/components/chat/InputBar.tsx`**

```tsx
import { useState, useCallback, type KeyboardEvent } from "react";
import { Button } from "@/components/ui/button";
import { Send } from "lucide-react";

interface InputBarProps {
  onSend: (text: string) => void;
  disabled: boolean;
  placeholder?: string;
}

export function InputBar({ onSend, disabled, placeholder }: InputBarProps) {
  const [text, setText] = useState("");

  const handleSend = useCallback(() => {
    const trimmed = text.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setText("");
  }, [text, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  return (
    <div className="flex items-end gap-2 p-3 border-t border-border bg-background">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        placeholder={placeholder ?? "Ask Claude anything..."}
        rows={1}
        className="flex-1 resize-none rounded-md border border-input bg-transparent px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50"
      />
      <Button
        size="icon"
        onClick={handleSend}
        disabled={disabled || !text.trim()}
        className="shrink-0"
      >
        <Send size={16} />
      </Button>
    </div>
  );
}
```

- [x] **Step 5: Write `src/components/chat/SessionIndicator.tsx`**

```tsx
import { cn } from "@/lib/utils";
import type { ClaudeSessionStatus } from "@/types/claude";

interface SessionIndicatorProps {
  status: ClaudeSessionStatus;
  model: string | null;
  costUsd: number;
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
}: SessionIndicatorProps) {
  const config = STATUS_CONFIG[status];

  return (
    <div className="flex items-center gap-3 px-4 py-2 border-b border-border text-xs text-muted-foreground">
      <div className="flex items-center gap-1.5">
        <div className={cn("w-2 h-2 rounded-full", config.color)} />
        <span>{config.label}</span>
      </div>
      {model && <span className="hidden sm:inline">{model}</span>}
      {costUsd > 0 && (
        <span className="ml-auto">${costUsd.toFixed(4)}</span>
      )}
    </div>
  );
}
```

- [x] **Step 6: Commit**

```bash
git add src/components/chat/
git commit -m "feat(claude): add chat UI components (MessageBubble, ToolActivityCard, InputBar, SessionIndicator)"
```

---

## Task 12: ChatView (Main View)

**Files:**
- Create: `src/views/ChatView.tsx`
- Delete: `src/views/ClaudeView.tsx`
- Delete: `src/hooks/useClaude.ts`
- Modify: `src/Router.tsx`

- [x] **Step 1: Write `src/views/ChatView.tsx`**

```tsx
import { useEffect, useRef, useCallback } from "react";
import { Bot } from "lucide-react";
import { Button } from "@/components/ui/button";
import { MessageBubble } from "@/components/chat/MessageBubble";
import { ToolActivityCard } from "@/components/chat/ToolActivityCard";
import { InputBar } from "@/components/chat/InputBar";
import { SessionIndicator } from "@/components/chat/SessionIndicator";
import { useClaudeStream } from "@/hooks/useClaudeStream";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import {
  claudeAuthStatus,
  claudeVersionCheck,
  spawnClaudeSession,
  sendClaudeMessage,
} from "@/lib/tauri-commands";

export default function ChatView() {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId)
  );

  const sessionId = useClaudeStore((s) => s.sessionId);
  const status = useClaudeStore((s) => s.status);
  const messages = useClaudeStore((s) => s.messages);
  const activeTools = useClaudeStore((s) => s.activeTools);
  const totalCostUsd = useClaudeStore((s) => s.totalCostUsd);
  const model = useClaudeStore((s) => s.model);
  const error = useClaudeStore((s) => s.error);

  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Subscribe to Tauri events
  useClaudeStream();

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, activeTools]);

  const handleConnect = useCallback(async () => {
    if (!engagement) return;

    // Preflight checks
    const version = await claudeVersionCheck();
    if (!version.installed) {
      useClaudeStore.getState().setError(
        "Claude CLI not found. Please install Claude Code first."
      );
      return;
    }
    if (!version.meets_minimum) {
      useClaudeStore.getState().setError(
        `Claude CLI ${version.version} is too old. Please update to ${version.version} or later.`
      );
      return;
    }

    const auth = await claudeAuthStatus();
    if (!auth.loggedIn) {
      useClaudeStore.getState().setError(
        "Not signed in to Claude. Please sign in first from Settings."
      );
      return;
    }

    useClaudeStore.getState().reset();
    useClaudeStore.setState({ status: "connecting" });

    try {
      await spawnClaudeSession(engagement.id, engagement.vault.path);
    } catch (e) {
      useClaudeStore.getState().setError(
        e instanceof Error ? e.message : String(e)
      );
    }
  }, [engagement]);

  const handleSend = useCallback(
    async (text: string) => {
      if (!sessionId) return;
      useClaudeStore.getState().addUserMessage(text);
      try {
        await sendClaudeMessage(sessionId, text);
      } catch (e) {
        useClaudeStore.getState().setError(
          e instanceof Error ? e.message : String(e)
        );
      }
    },
    [sessionId]
  );

  // No engagement selected
  if (!activeEngagementId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Bot size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to use Claude.</p>
      </div>
    );
  }

  // Not connected yet
  if (status === "disconnected" && !error) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <Bot size={48} className="text-muted-foreground" />
        <p className="text-sm text-muted-foreground">
          Start a Claude session for this engagement
        </p>
        <Button onClick={handleConnect}>Connect to Claude</Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <SessionIndicator status={status} model={model} costUsd={totalCostUsd} />

      {/* Messages area */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {/* Tool activity cards (shown between messages) */}
        {activeTools
          .filter((t) => t.status === "running")
          .map((tool) => (
            <ToolActivityCard key={tool.toolId} tool={tool} />
          ))}

        {error && (
          <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive text-sm">
            <span>{error}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={handleConnect}
              className="ml-auto"
            >
              Retry
            </Button>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      <InputBar
        onSend={handleSend}
        disabled={status === "thinking" || status === "disconnected" || status === "connecting"}
        placeholder={
          status === "thinking"
            ? "Claude is thinking..."
            : "Ask Claude anything..."
        }
      />
    </div>
  );
}
```

- [x] **Step 2: Delete old files**

```bash
rm src/views/ClaudeView.tsx
rm src/hooks/useClaude.ts
```

- [x] **Step 3: Update `src/Router.tsx`**

Change the lazy import from `ClaudeView` to `ChatView`:

Replace:
```typescript
const ClaudeView = lazy(() => import("@/views/ClaudeView"));
```
With:
```typescript
const ChatView = lazy(() => import("@/views/ChatView"));
```

And in the `VIEW_MAP`, replace:
```typescript
claude: { component: ClaudeView, label: "Claude Code" },
```
With:
```typescript
claude: { component: ChatView, label: "Claude" },
```

- [x] **Step 4: Verify TypeScript compiles**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | head -30`
Expected: No errors from ChatView or Router.

- [x] **Step 5: Run all tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run 2>&1 | tail -20`
Expected: All tests pass (including the 8 new claudeStore tests).

- [x] **Step 6: Commit**

```bash
git add -A src/views/ src/hooks/ src/Router.tsx
git commit -m "feat(claude): replace ClaudeView with ChatView — full embedded chat experience"
```

---

## Task 13: Full Build Verification

**Files:** None (verification only)

- [x] **Step 1: Run Rust build**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo build 2>&1 | tail -20`
Expected: Build succeeds.

- [x] **Step 2: Run TypeScript typecheck**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [x] **Step 3: Run all tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run 2>&1 | tail -20`
Expected: All tests pass.

- [x] **Step 4: Verify no references to old M1 code remain**

Run: `grep -r "claude_preflight\|scaffold_claude_project\|launch_claude\|ClaudeView\|useClaude" src/ src-tauri/src/ --include="*.ts" --include="*.tsx" --include="*.rs" 2>&1`
Expected: No matches (all old M1 references removed).

- [x] **Step 5: Commit (if any fixes were needed)**

```bash
git add -A
git commit -m "chore: phase 1 build verification — all clean"
```

---

## Self-Review Checklist

| Spec Section | Task(s) | Covered? |
|-------------|---------|----------|
| 3.2 CLI Subprocess Protocol | T2, T3 | Yes — spawn args, stream parser, hook filtering |
| 3.2.1 Hook Filtering Strategy | T2 | Yes — system hook events silently dropped |
| Stream Parser Translation Table | T2 | Yes — all 12 event types handled |
| Friendly Labels for Tools | T2 | Yes — friendly_label() function |
| 3.4 Authentication | T4 | Yes — auth status + login commands |
| 3.10 Rust Backend: New Commands | T4, T5, T6 | Yes — all 6 commands created, old 3 removed |
| 3.11 React Frontend: New Components | T8, T9, T10, T11, T12 | Yes — store, hook, 4 components, ChatView |
| 3.14 Process Health & Crash Recovery | T3 | Yes — monitor_process, classify_exit |
| 3.16 What This Replaces | T6, T12 | Yes — old files deleted, Router updated |
| Risk R1 (CLI version check) | T4 | Yes — claude_version_check with semver compare |
| Risk R9 (permission handling) | Not yet | Deferred — spec says test in Phase 1, will need a follow-up task |

**Note:** Permission mode testing (R9) requires a running Claude CLI with an active engagement folder. This should be done as a manual integration test after the build is verified, not as part of the automated plan.

---

Plan complete and saved to `docs/superpowers/plans/2026-04-11-m2-embedded-claude-phase1.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
