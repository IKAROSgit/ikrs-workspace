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

#[cfg(target_family = "unix")]
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_family = "windows")]
fn is_process_alive(_pid: u32) -> bool {
    false
}

#[cfg(target_family = "unix")]
fn is_claude_process(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains("claude"),
        Err(_) => false,
    }
}

#[cfg(target_family = "windows")]
fn is_claude_process(_pid: u32) -> bool {
    false
}

#[cfg(target_family = "unix")]
fn kill_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}

#[cfg(target_family = "windows")]
fn kill_process(_pid: u32) {
    // No-op: Windows orphan cleanup deferred to future phase
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
        let dir =
            std::env::temp_dir().join(format!("ikrs-registry-test-{}", uuid::Uuid::new_v4()));
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
