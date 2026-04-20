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
            if s.chars().count() > 80 {
                format!("{}...", s.chars().take(77).collect::<String>())
            } else {
                s.clone()
            }
        }
        Some(_) => "Completed".to_string(),
        None => "Completed".to_string(),
    }
}

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

/// Diagnostic log file. 2026-04-19 addition to debug the
/// "Connecting..." wedge — the webview's DevTools wouldn't open and
/// we needed out-of-process visibility into what the Rust parser
/// was actually receiving from the Claude CLI subprocess. The file
/// is truncated on each parser start. Kept in `/tmp` (ephemeral) so
/// it doesn't leak user data between sessions.
fn dbg_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ikrs-stream.log")
    {
        let ts = chrono::Utc::now().format("%H:%M:%S%.3f");
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

/// Reads Claude CLI stdout line-by-line and emits typed Tauri events.
/// Returns when the stream ends (process exited or pipe broken).
///
/// `internal_session_id` is the uuid we assigned when spawning.
/// Claude's stream emits a DIFFERENT session_id in its `system:init`
/// frame (claude generates its own). Without this param the parser
/// would emit session-ready events carrying claude's id — which
/// diverges from the key we use in the session HashMap and breaks
/// subsequent send_claude_message calls. Pass ours in so init-
/// triggered session-ready events reuse it.
pub async fn parse_stream(
    stdout: ChildStdout,
    app: AppHandle,
    internal_session_id: String,
) {
    // Debug log is APPENDED across parse_stream invocations (one per
    // spawn) so we don't lose history of prior sessions when the app
    // re-spawns after a crash. Truncated only when the whole app
    // restarts via the external `rm -f` on launch.
    dbg_log("==================== parse_stream started ====================");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut msg_id_gen = MessageIdGen::new();
    let mut current_msg_id = msg_id_gen.next();
    let mut tool_name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Separate map tracking Write/Edit/NotebookEdit tool_id →
    // (file_path, pre-write mtime) so we can stat the file on
    // tool_result and prove whether the write actually landed on
    // disk. Claude's reported success is NOT trusted — only the
    // filesystem.
    let mut write_targets: std::collections::HashMap<String, WriteTarget> =
        std::collections::HashMap::new();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                // Log FULL line (not preview) so we can diagnose
                // malformed inputs / errors post-mortem.
                dbg_log(&format!("LINE: {line}"));
                handle_line(
                    &line,
                    &app,
                    &mut msg_id_gen,
                    &mut current_msg_id,
                    &mut tool_name_map,
                    &mut write_targets,
                    &internal_session_id,
                );
            }
            Ok(None) => {
                dbg_log("EOF — process exited or pipe closed");
                break;
            }
            Err(e) => {
                dbg_log(&format!("stream error: {e}"));
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
    dbg_log("parse_stream exiting");
}

/// Tracks a pending Write/Edit/NotebookEdit tool_use so we can
/// verify on the corresponding tool_result whether the file
/// actually landed on disk. Never trust Claude's self-reported
/// success — stat the file.
pub(super) struct WriteTarget {
    pub tool_name: String,
    pub path: String,
    /// Pre-write mtime (None if the file didn't exist before).
    /// Used for Edit / NotebookEdit: a successful Edit MUST bump
    /// mtime; if it hasn't moved, the edit was a no-op or failed.
    pub prev_mtime_secs: Option<i64>,
}

/// Stat a write target and produce the verification payload.
///
/// Rules:
///  - Write: file must exist AND have non-zero size.
///  - Edit / NotebookEdit: file must exist AND mtime must be >= the
///    pre-write mtime (strictly > is over-strict because filesystems
///    can round to whole seconds; equal-second is acceptable).
///
/// `claude_claimed_success` is recorded so the frontend can detect
/// and highlight the lie class of bug (Claude says saved, disk
/// disagrees).
fn verify_write(
    target: &WriteTarget,
    claude_claimed_success: bool,
) -> WriteVerificationPayload {
    let meta = match std::fs::metadata(&target.path) {
        Ok(m) => m,
        Err(e) => {
            return WriteVerificationPayload {
                tool_id: String::new(), // filled by caller? no — we pass it in emit
                tool_name: target.tool_name.clone(),
                path: target.path.clone(),
                verified: false,
                size_bytes: None,
                reason: Some(format!("file does not exist on disk: {e}")),
                claude_claimed_success,
            };
        }
    };
    let size = meta.len();
    let now_mtime = meta
        .modified()
        .ok()
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64)
        });

    let (verified, reason) = match target.tool_name.as_str() {
        "Write" => {
            if size == 0 {
                (false, Some("file exists but is empty (size=0)".to_string()))
            } else {
                (true, None)
            }
        }
        "Edit" | "NotebookEdit" => match (target.prev_mtime_secs, now_mtime) {
            (Some(prev), Some(cur)) if cur >= prev => (true, None),
            (Some(prev), Some(cur)) => (
                false,
                Some(format!("mtime did not advance (prev={prev}, cur={cur})")),
            ),
            (None, Some(_)) => (true, None), // edit that created the file
            (_, None) => (false, Some("could not read mtime".to_string())),
        },
        _ => (
            false,
            Some(format!("unknown write-tool name '{}'", target.tool_name)),
        ),
    };

    WriteVerificationPayload {
        tool_id: String::new(),
        tool_name: target.tool_name.clone(),
        path: target.path.clone(),
        verified,
        size_bytes: Some(size),
        reason,
        claude_claimed_success,
    }
}

fn handle_line(
    line: &str,
    app: &AppHandle,
    msg_id_gen: &mut MessageIdGen,
    current_msg_id: &mut String,
    tool_name_map: &mut std::collections::HashMap<String, String>,
    write_targets: &mut std::collections::HashMap<String, WriteTarget>,
    internal_session_id: &str,
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
        "system" => handle_system_event(&raw, app, internal_session_id),
        "assistant" => handle_assistant_event(
            &raw,
            app,
            msg_id_gen,
            current_msg_id,
            tool_name_map,
            write_targets,
        ),
        "user" => handle_user_event(&raw, app, tool_name_map, write_targets),
        "result" => handle_result_event(&raw, app),
        "rate_limit_event" => { /* silently drop — internal bookkeeping */ }
        _ => {
            log::debug!("Unknown stream event type: {}", event_type);
        }
    }
}

fn handle_system_event(
    raw: &serde_json::Value,
    app: &AppHandle,
    internal_session_id: &str,
) {
    let subtype = raw["subtype"].as_str().unwrap_or("");
    match subtype {
        "hook_started" | "hook_response" => {
            // Silently filtered — consultant doesn't need to see hook lifecycle
            log::debug!("Filtered hook event: {}", subtype);
        }
        "init" => {
            // Use the session_id we assigned at spawn time, NOT the
            // claude-generated one in raw["session_id"]. Rust's
            // ClaudeSessionManager.sessions HashMap is keyed by our
            // internal uuid; sending a message after the real init
            // would fail ("Session not found") if the frontend
            // switched its sessionId to claude's uuid here.
            let payload = SessionReadyPayload {
                session_id: internal_session_id.to_string(),
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
            dbg_log(&format!(
                "EMIT claude:session-ready session_id={} model={} tools_n={}",
                payload.session_id,
                payload.model,
                payload.tools.len()
            ));
            let result = app.emit("claude:session-ready", &payload);
            dbg_log(&format!(
                "emit result: {:?}",
                result.as_ref().map(|_| "ok").map_err(|e| e.to_string())
            ));
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
    tool_name_map: &mut std::collections::HashMap<String, String>,
    write_targets: &mut std::collections::HashMap<String, WriteTarget>,
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
                        tool_input: serialize_tool_input(input),
                    },
                );
                tool_name_map.insert(tool_id.to_string(), tool_name.to_string());

                // If this is a file-writing tool, capture the target
                // path + pre-write mtime so the tool_result handler
                // can stat it and independently verify the write.
                if matches!(tool_name, "Write" | "Edit" | "NotebookEdit") {
                    if let Some(path) = input["file_path"].as_str() {
                        let prev_mtime_secs = std::fs::metadata(path)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .ok()
                                    .map(|d| d.as_secs() as i64)
                            });
                        write_targets.insert(
                            tool_id.to_string(),
                            WriteTarget {
                                tool_name: tool_name.to_string(),
                                path: path.to_string(),
                                prev_mtime_secs,
                            },
                        );
                    }
                }
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

fn handle_user_event(
    raw: &serde_json::Value,
    app: &AppHandle,
    tool_name_map: &std::collections::HashMap<String, String>,
    write_targets: &mut std::collections::HashMap<String, WriteTarget>,
) {
    // User events contain tool_result content blocks
    let content = match raw["message"]["content"].as_array() {
        Some(arr) => arr,
        None => return,
    };

    for block in content {
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

            // Ground-truth verification: if this tool_result is for a
            // Write/Edit/NotebookEdit we were tracking, stat the file
            // and emit claude:write-verified with the actual disk
            // state. Claude's self-reported `is_error` is recorded
            // but NOT trusted — the payload exposes both so the UI
            // can highlight the mismatch loudly.
            if let Some(target) = write_targets.remove(tool_id) {
                let claude_claimed_success = !is_error;
                let mut verification =
                    verify_write(&target, claude_claimed_success);
                verification.tool_id = tool_id.to_string();
                dbg_log(&format!(
                    "WRITE-VERIFY tool_id={tool_id} name={} path={} verified={} size={:?} claimed_success={} reason={:?}",
                    verification.tool_name,
                    verification.path,
                    verification.verified,
                    verification.size_bytes,
                    verification.claude_claimed_success,
                    verification.reason,
                ));
                let _ = app.emit("claude:write-verified", &verification);

                // If Claude claimed success but the stat contradicts
                // it, also emit a user-visible error so the UI can
                // surface the lie non-dismissibly.
                if verification.claude_claimed_success && !verification.verified {
                    let _ = app.emit(
                        "claude:error",
                        ErrorPayload {
                            message: format!(
                                "Claude reported '{}' success for {} but the file did not land on disk ({}). Your content was NOT saved.",
                                verification.tool_name,
                                verification.path,
                                verification.reason.clone().unwrap_or_else(|| "no reason".to_string()),
                            ),
                        },
                    );
                }
            }

            // Auth-error detection for MCP tools
            if is_error {
                if let Some(content_val) = &content_ref {
                    let content_str = match content_val {
                        serde_json::Value::String(s) => s.clone(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    if is_auth_error(&content_str) {
                        let resolved_name = tool_name_map.get(tool_id).map(|s| s.as_str()).unwrap_or("");
                        let server = infer_mcp_server(resolved_name);
                        let _ = app.emit(
                            "claude:mcp-auth-error",
                            McpAuthErrorPayload {
                                server_name: server,
                                error_hint: cap_string(&content_str, 200),
                            },
                        );
                    }
                }
            }
        }
    }
}

/// Check if an error message indicates an authentication failure.
fn is_auth_error(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("401")
        || lower.contains("403")
        || lower.contains("token expired")
        || lower.contains("authentication failed")
        || lower.contains("invalid_grant")
        || lower.contains("unauthenticated")
}

/// Infer which MCP server a tool belongs to from the tool_name.
/// MCP tools are prefixed: mcp__gmail__*, mcp__calendar__*, etc.
fn infer_mcp_server(tool_name: &str) -> String {
    if tool_name.starts_with("mcp__gmail__") {
        return "gmail".to_string();
    }
    if tool_name.starts_with("mcp__calendar__") {
        return "calendar".to_string();
    }
    if tool_name.starts_with("mcp__drive__") {
        return "drive".to_string();
    }
    if tool_name.starts_with("mcp__obsidian__") {
        return "obsidian".to_string();
    }
    "unknown".to_string()
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

    #[test]
    fn test_is_auth_error_401() {
        assert!(is_auth_error("HTTP 401 Unauthorized"));
    }

    #[test]
    fn test_is_auth_error_token_expired() {
        assert!(is_auth_error("The token expired at 2026-04-12T10:00:00Z"));
    }

    #[test]
    fn test_is_auth_error_normal() {
        assert!(!is_auth_error("File not found: /some/path"));
    }

    #[test]
    fn test_infer_mcp_server_gmail() {
        assert_eq!(infer_mcp_server("mcp__gmail__read_message"), "gmail");
    }

    #[test]
    fn test_infer_mcp_server_unknown() {
        assert_eq!(infer_mcp_server("Read"), "unknown");
    }

    // ---------- write-verification tests (2026-04-20) ----------
    // The lie-class-of-bug: Claude's assistant text says "saved to
    // ..." but the file was never written. verify_write must catch
    // every flavour of that lie — missing file, empty file, stale
    // mtime on Edit.

    fn make_tmp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("ikrs-verify-{}-{}", label, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_verify_write_nonexistent_file() {
        let tmp = make_tmp_dir("missing");
        let target = WriteTarget {
            tool_name: "Write".to_string(),
            path: tmp.join("never-landed.md").to_string_lossy().to_string(),
            prev_mtime_secs: None,
        };
        let v = verify_write(&target, true);
        assert!(!v.verified, "missing file must not verify");
        assert_eq!(v.claude_claimed_success, true);
        let reason = v.reason.unwrap_or_default();
        assert!(
            reason.contains("does not exist"),
            "reason should say file missing, got: {reason}"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_verify_write_empty_file_is_not_verified() {
        let tmp = make_tmp_dir("empty");
        let path = tmp.join("empty.md");
        std::fs::write(&path, "").unwrap();
        let target = WriteTarget {
            tool_name: "Write".to_string(),
            path: path.to_string_lossy().to_string(),
            prev_mtime_secs: None,
        };
        let v = verify_write(&target, true);
        assert!(!v.verified, "empty file must not verify as Write");
        assert_eq!(v.size_bytes, Some(0));
        assert!(v.reason.unwrap_or_default().contains("empty"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_verify_write_nonempty_file_verifies() {
        let tmp = make_tmp_dir("ok");
        let path = tmp.join("hello.md");
        std::fs::write(&path, "hello world").unwrap();
        let target = WriteTarget {
            tool_name: "Write".to_string(),
            path: path.to_string_lossy().to_string(),
            prev_mtime_secs: None,
        };
        let v = verify_write(&target, true);
        assert!(v.verified, "non-empty file should verify");
        assert_eq!(v.size_bytes, Some(11));
        assert!(v.reason.is_none());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_verify_edit_requires_mtime_advance() {
        let tmp = make_tmp_dir("edit-stale");
        let path = tmp.join("edit.md");
        std::fs::write(&path, "original").unwrap();
        let prev = std::fs::metadata(&path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Simulate Edit that didn't actually bump mtime (permission
        // denial on a file that already existed). Mark prev_mtime as
        // NOW so a same-second stat still equals — acceptable per
        // filesystem second-rounding. But artificially advance
        // prev_mtime so we test the strict fail path.
        let target = WriteTarget {
            tool_name: "Edit".to_string(),
            path: path.to_string_lossy().to_string(),
            prev_mtime_secs: Some(prev + 3600), // pretend it was last edited in the future
        };
        let v = verify_write(&target, true);
        assert!(!v.verified, "Edit with stale mtime must fail");
        assert!(v.reason.unwrap_or_default().contains("mtime"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_verify_edit_success_when_mtime_advances() {
        let tmp = make_tmp_dir("edit-ok");
        let path = tmp.join("edit.md");
        std::fs::write(&path, "original").unwrap();
        let prev = std::fs::metadata(&path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Sleep briefly then rewrite so mtime advances by >= 1 sec
        // on filesystems that round to whole seconds.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&path, "edited content").unwrap();

        let target = WriteTarget {
            tool_name: "Edit".to_string(),
            path: path.to_string_lossy().to_string(),
            prev_mtime_secs: Some(prev),
        };
        let v = verify_write(&target, true);
        assert!(v.verified, "Edit with advanced mtime should verify");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_verify_records_lie_when_claude_claims_success_but_file_missing() {
        // Data-loss regression guard: Moe 2026-04-20 lost a triaged
        // tracker + transcript because Claude's assistant text said
        // "saved" while the permission denial auto-dismissed and the
        // file was never written. Payload MUST expose both
        // claude_claimed_success=true and verified=false so the UI
        // can shout about the lie.
        let tmp = make_tmp_dir("lie");
        let target = WriteTarget {
            tool_name: "Write".to_string(),
            path: tmp.join("ghost.md").to_string_lossy().to_string(),
            prev_mtime_secs: None,
        };
        let v = verify_write(&target, true);
        assert!(v.claude_claimed_success);
        assert!(!v.verified);
        // This is the inequality the UI flags red.
        assert!(
            v.claude_claimed_success != v.verified,
            "lie detector failed to fire"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }
}
