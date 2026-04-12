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

        let mut child = Command::new("claude")
            .args(&args)
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
