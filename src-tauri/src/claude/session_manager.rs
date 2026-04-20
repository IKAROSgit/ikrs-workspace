use crate::claude::stream_parser::parse_stream;
use crate::claude::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
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
    /// Returns (session_id, child_pid).
    pub async fn spawn(
        &self,
        engagement_id: String,
        engagement_path: String,
        resume_session_id: Option<String>,
        env_vars: HashMap<String, String>,
        mcp_config_path: Option<String>,
        app: AppHandle,
    ) -> Result<(String, u32), String> {
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

        if let Some(ref config_path) = mcp_config_path {
            args.push("--mcp-config".to_string());
            args.push(config_path.clone());
            // Strict mode: don't merge user-level / system-level MCP
            // configs with our per-engagement one. Without this flag
            // Claude CLI inherits the consultant's personal MCPs
            // (e.g. chrome-devtools-mcp, mcp-remote to custom
            // endpoints), which break the subprocess in two ways:
            //   1. They're designed for the desktop Claude.app
            //      environment and may hang when started from a Tauri
            //      subprocess with minimal env (proven 2026-04-18 on
            //      Moe's Mac — chrome-devtools + mcp-remote hung for
            //      54 minutes, blocking system.init forever).
            //   2. They create per-client NDA leak risk — BLR's Claude
            //      should not have access to consultant's personal
            //      Nimble MCP or similar.
            //
            // Codex Phase 3 scope review (2026-04-11) recommended
            // non-strict as default. That recommendation was wrong for
            // this product: per-engagement isolation is a hard
            // requirement, not a policy knob. Forcing strict is the
            // correct call; individual consultants who want their
            // user-level MCPs can still use Claude Code directly
            // outside the workspace app.
            args.push("--strict-mcp-config".to_string());
        }

        // Resolve claude binary path from app state (resolved at startup)
        let resolved: tauri::State<'_, crate::claude::binary_resolver::ResolvedBinaries> =
            app.state();
        let claude_path = resolved
            .claude
            .as_ref()
            .ok_or("Claude CLI not found. Please install Claude Code (https://claude.ai/code).")?;

        // Prepend resolved binary directories to existing PATH so Claude CLI
        // can find npx/node under macOS App Sandbox (where PATH is restricted).
        let resolved_path = resolved.to_path_env();
        let existing_path = std::env::var("PATH").unwrap_or_default();
        let full_path = if resolved_path.is_empty() {
            existing_path
        } else if existing_path.is_empty() {
            resolved_path
        } else {
            let sep = if cfg!(target_family = "windows") { ";" } else { ":" };
            format!("{resolved_path}{sep}{existing_path}")
        };

        // Note: .envs() is additive — it adds to the inherited environment, not replaces it.
        let mut child = Command::new(claude_path)
            .args(&args)
            .current_dir(&engagement_path)
            .env("PATH", full_path)
            .envs(&env_vars)
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

        // Emit synthetic session-ready immediately on spawn success.
        //
        // Diagnosed 2026-04-20 via direct Mac SSH after hours of
        // chasing frontend race theories: `claude --print
        // --input-format stream-json` does NOT emit the `system:init`
        // frame (from which we derive `claude:session-ready`) until
        // it receives the FIRST user message on stdin. Until then
        // claude sits idle after running SessionStart hooks, which
        // means:
        //   1. UI shows "Connecting..." waiting for session-ready
        //   2. User can't type the first message because the input
        //      is disabled while status is "connecting"
        //   3. Deadlock.
        //
        // The fix: synthesize a session-ready event from what we
        // already know at spawn time — session_id, an empty tools
        // list, and the model placeholder "initializing". The UI
        // unwedges and lets the consultant type. When the real
        // `system:init` arrives (triggered by that first user
        // message), `setSessionReady` overwrites with the actual
        // tools array + model name. The MCP store gets populated
        // from the real init.
        //
        // Cost: tools/MCP badges show empty for the first few
        // seconds after a fresh spawn, until the first send. Small
        // price for unwedging daily use.
        let _ = app.emit(
            "claude:session-ready",
            SessionReadyPayload {
                session_id: session_id.clone(),
                tools: Vec::new(),
                model: "initializing".to_string(),
                cwd: engagement_path.clone(),
            },
        );

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

        // Capture child PID before moving into monitor (for registry)
        let child_pid = child.id().unwrap_or(0);

        // Spawn process monitor task (detects crashes)
        let monitor_app = app.clone();
        let monitor_session_id = session_id.clone();
        let monitor_engagement_id = engagement_id.clone();
        let monitor_sessions = Arc::clone(&self.sessions);
        tokio::spawn(async move {
            monitor_process(child, monitor_session_id, monitor_engagement_id, monitor_sessions, monitor_app).await;
        });

        // Store session
        let session = ClaudeSession {
            stdin,
            session_id: session_id.clone(),
            engagement_id,
        };
        self.sessions.lock().await.insert(session_id.clone(), session);

        Ok((session_id, child_pid))
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
    /// Returns the engagement_id of the killed session (for registry cleanup).
    pub async fn kill(&self, session_id: &str) -> Result<String, String> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .remove(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;
        // Dropping the session closes stdin, which signals EOF to the CLI process.
        // The monitor task will detect the exit and emit claude:session-ended.
        Ok(session.engagement_id)
    }

    /// Check if a session is active.
    pub async fn has_session(&self) -> bool {
        !self.sessions.lock().await.is_empty()
    }
}

/// Monitors a Claude child process and emits events on exit.
async fn monitor_process(
    mut child: Child,
    session_id: String,
    engagement_id: String,
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
                // C2 fix: Unregister from file registry on exit
                if let Ok(app_data_dir) = app.path().app_data_dir() {
                    let _ = crate::claude::registry::unregister_session(&app_data_dir, &engagement_id);
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
                // C2 fix: Unregister from file registry on error
                if let Ok(app_data_dir) = app.path().app_data_dir() {
                    let _ = crate::claude::registry::unregister_session(&app_data_dir, &engagement_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_removed_after_kill() {
        let manager = ClaudeSessionManager::new();
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
            manager
                .sessions
                .lock()
                .await
                .insert("test-sess".to_string(), session);
        }
        assert!(manager.has_session().await);
        let eng_id = manager.kill("test-sess").await.unwrap();
        assert_eq!(eng_id, "test-eng");
        assert!(!manager.has_session().await);
    }
}
