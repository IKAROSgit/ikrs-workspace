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

/// Reads Claude CLI stdout line-by-line and emits typed Tauri events.
/// Returns when the stream ends (process exited or pipe broken).
pub async fn parse_stream(stdout: ChildStdout, app: AppHandle) {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut msg_id_gen = MessageIdGen::new();
    let mut current_msg_id = msg_id_gen.next();
    let mut tool_name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                handle_line(&line, &app, &mut msg_id_gen, &mut current_msg_id, &mut tool_name_map);
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
    tool_name_map: &mut std::collections::HashMap<String, String>,
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
        "assistant" => handle_assistant_event(&raw, app, msg_id_gen, current_msg_id, tool_name_map),
        "user" => handle_user_event(&raw, app, tool_name_map),
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
    tool_name_map: &mut std::collections::HashMap<String, String>,
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

fn handle_user_event(raw: &serde_json::Value, app: &AppHandle, tool_name_map: &std::collections::HashMap<String, String>) {
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
}
