//! Vault filesystem watcher that bridges Claude's markdown task
//! writes into the Kanban board.
//!
//! Flow (Codex 2026-04-21 §B.5 Option A):
//!   1. Frontend calls `start_task_watch(engagement_id, client_slug)`
//!      when entering TasksView.
//!   2. This module spawns a `notify::RecommendedWatcher` on
//!      `<vault>/02-tasks/`. Events are debounced 250ms per path.
//!   3. On a markdown write, we parse YAML frontmatter (id, title,
//!      status, priority, etc.) and emit a `task:vault-change`
//!      Tauri event with the parsed struct.
//!   4. Frontend listens and calls `updateTask` / `createTask` on
//!      Firestore. The anti-flicker 250ms local-pending-edits
//!      guard in the frontend prevents mid-drag UI snap-back.
//!
//! The watcher handle is stored in Tauri state so repeat calls
//! (e.g. on engagement switch) replace it cleanly. Stopping the
//! watcher is automatic when the state is replaced or dropped.

use notify::{
    event::{EventKind, ModifyKind},
    Event, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::vault::vault_path_for_slug;

/// Shared mutable state holding the active watcher, if any. One
/// watcher per consultant (one active engagement at a time, per
/// product decision + `max_sessions=1`).
#[derive(Default)]
pub struct TaskWatchState(pub Mutex<Option<ActiveWatcher>>);

pub struct ActiveWatcher {
    pub engagement_id: String,
    pub tasks_dir: PathBuf,
    #[allow(dead_code)] // held for its Drop
    watcher: RecommendedWatcher,
    #[allow(dead_code)]
    debounce: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

/// Parsed frontmatter a Claude-written task markdown file carries.
/// Matches the shape the frontend expects on `task:vault-change`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskFrontmatter {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub client_visible: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_assignee")]
    pub assignee: String,
    /// Relative path inside the vault — e.g. "02-tasks/abc123.md"
    /// Filled in by the watcher, not by Claude.
    #[serde(default)]
    pub vault_path: String,
    /// Engagement id for the frontend to scope the Firestore write.
    /// Filled by the watcher, not Claude.
    #[serde(default)]
    pub engagement_id: String,
}

fn default_priority() -> String {
    "p2".to_string()
}
fn default_assignee() -> String {
    "claude".to_string()
}

/// Start watching `<vault>/02-tasks/` for this engagement. Replaces
/// any previous watcher. Idempotent on repeat calls with the same
/// engagement_id (no-ops).
#[tauri::command]
pub fn start_task_watch(
    engagement_id: String,
    client_slug: String,
    app: AppHandle,
    state: State<'_, TaskWatchState>,
) -> Result<(), String> {
    let vault_root = vault_path_for_slug(&client_slug)?;
    let tasks_dir = vault_root.join("02-tasks");
    if !tasks_dir.exists() {
        std::fs::create_dir_all(&tasks_dir)
            .map_err(|e| format!("create 02-tasks: {e}"))?;
    }

    // If we're already watching the same directory for the same
    // engagement, no-op.
    {
        let guard = state.0.lock().unwrap();
        if let Some(active) = guard.as_ref() {
            if active.engagement_id == engagement_id
                && active.tasks_dir == tasks_dir
            {
                return Ok(());
            }
        }
    }

    let app_for_cb = app.clone();
    let engagement_id_for_cb = engagement_id.clone();
    let tasks_dir_for_cb = tasks_dir.clone();
    let debounce: Arc<Mutex<HashMap<PathBuf, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let debounce_for_cb = Arc::clone(&debounce);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        let event = match res {
            Ok(e) => e,
            Err(e) => {
                log::warn!("task_watch notify error: {e}");
                return;
            }
        };
        // We only care about creates + modifies to .md files.
        let should_handle = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Any)
        );
        if !should_handle {
            return;
        }
        for path in event.paths {
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            // Debounce per-path @ 250ms — notify can fire multiple
            // events for a single logical save (metadata + data).
            {
                let mut map = debounce_for_cb.lock().unwrap();
                let now = Instant::now();
                if let Some(prev) = map.get(&path) {
                    if now.duration_since(*prev) < Duration::from_millis(250) {
                        continue;
                    }
                }
                map.insert(path.clone(), now);
            }

            // Read + parse frontmatter. Silently drop malformed
            // files — they'll appear in the next notify event
            // after the user / Claude fixes them.
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    log::debug!("task_watch read {path:?}: {e}");
                    continue;
                }
            };
            let parsed = match parse_frontmatter(&content) {
                Ok(p) => p,
                Err(e) => {
                    log::debug!("task_watch frontmatter parse {path:?}: {e}");
                    continue;
                }
            };
            let rel = path
                .strip_prefix(tasks_dir_for_cb.parent().unwrap_or(&tasks_dir_for_cb))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| {
                    path.file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default()
                });

            let payload = TaskFrontmatter {
                vault_path: rel,
                engagement_id: engagement_id_for_cb.clone(),
                ..parsed
            };
            let _ = app_for_cb.emit("task:vault-change", &payload);
        }
    })
    .map_err(|e| format!("notify watcher init: {e}"))?;

    watcher
        .watch(&tasks_dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("notify watch {tasks_dir:?}: {e}"))?;

    let active = ActiveWatcher {
        engagement_id,
        tasks_dir,
        watcher,
        debounce,
    };
    *state.0.lock().unwrap() = Some(active);
    Ok(())
}

/// Explicit stop for the active watcher. Not strictly required —
/// the watcher is also dropped when the state is replaced or the
/// app exits — but useful for tests and engagement switches.
#[tauri::command]
pub fn stop_task_watch(state: State<'_, TaskWatchState>) -> Result<(), String> {
    *state.0.lock().unwrap() = None;
    Ok(())
}

/// Parse the leading `---\n...\n---` YAML frontmatter from a
/// markdown file. Body is ignored here — the note body gets
/// persisted into Firestore's separate `taskNotes` collection by
/// the frontend bridge if a dedicated notes section is detected.
pub(crate) fn parse_frontmatter(md: &str) -> Result<TaskFrontmatter, String> {
    let trimmed = md.trim_start_matches('\u{feff}'); // BOM
    if !trimmed.starts_with("---") {
        return Err("no frontmatter".to_string());
    }
    let after_open = &trimmed[3..];
    let Some(end) = after_open.find("\n---") else {
        return Err("frontmatter not terminated".to_string());
    };
    let yaml = &after_open[..end].trim_start_matches('\n');
    let parsed: TaskFrontmatter = serde_yaml::from_str(yaml)
        .map_err(|e| format!("yaml parse: {e}"))?;
    if parsed.id.is_empty() {
        return Err("missing id".to_string());
    }
    if parsed.title.is_empty() {
        return Err("missing title".to_string());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_minimal() {
        let md = "---\nid: abc-123\ntitle: Send proposal\nstatus: in_progress\n---\nBody text";
        let fm = parse_frontmatter(md).unwrap();
        assert_eq!(fm.id, "abc-123");
        assert_eq!(fm.title, "Send proposal");
        assert_eq!(fm.status, "in_progress");
        assert_eq!(fm.priority, "p2"); // default
        assert_eq!(fm.assignee, "claude"); // default
    }

    #[test]
    fn test_parse_frontmatter_full() {
        let md = r#"---
id: t1
title: "Review contract"
status: in_review
priority: p1
tags: [legal, urgent]
due: 2026-05-01
client_visible: false
assignee: consultant
description: |
  Multi-line
  description
---
body"#;
        let fm = parse_frontmatter(md).unwrap();
        assert_eq!(fm.priority, "p1");
        assert_eq!(fm.tags, vec!["legal", "urgent"]);
        assert_eq!(fm.due.as_deref(), Some("2026-05-01"));
        assert_eq!(fm.client_visible, Some(false));
        assert_eq!(fm.assignee, "consultant");
        assert!(fm.description.as_deref().unwrap_or("").contains("Multi-line"));
    }

    #[test]
    fn test_parse_frontmatter_rejects_missing_id() {
        let md = "---\ntitle: x\nstatus: backlog\n---\n";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn test_parse_frontmatter_rejects_missing_title() {
        let md = "---\nid: x\nstatus: backlog\n---\n";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn test_parse_frontmatter_rejects_no_frontmatter() {
        let md = "# Just a heading\n\nnot a task file";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn test_parse_frontmatter_rejects_unterminated() {
        let md = "---\nid: x\ntitle: y\n\nbody without closing";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn test_parse_frontmatter_handles_bom() {
        let md = "\u{feff}---\nid: t1\ntitle: x\nstatus: backlog\n---\n";
        let fm = parse_frontmatter(md).unwrap();
        assert_eq!(fm.id, "t1");
    }
}
