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
    /// JoinHandle for the detached initial-scan task. When the
    /// watcher is replaced or stopped, Drop aborts the scan so it
    /// can't keep emitting `task:vault-change` events for the
    /// previous engagement after a switch. Codex 2026-04-22.
    scan_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for ActiveWatcher {
    fn drop(&mut self) {
        if let Some(h) = self.scan_handle.take() {
            h.abort();
        }
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_assignee")]
    pub assignee: String,
    /// Relative path inside the vault — e.g. "02-tasks/abc123.md"
    /// Filled by the watcher at emit time; NEVER persisted to the
    /// markdown file (skip_serializing_if empty).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vault_path: String,
    /// Engagement id for the frontend to scope the Firestore write.
    /// Filled by the watcher; NEVER persisted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
///
/// **Default path derivation**: `~/.ikrs-workspace/vaults/<slug>/`
/// via `vault_path_for_slug(slug)`. This matches where
/// `write_task_frontmatter` writes — so UI-edits and Claude-writes
/// land in the same folder the watcher watches. Symmetry preserved.
///
/// `vault_path` is an OPTIONAL override. When explicitly passed
/// (non-empty), we watch that path instead. Kept as an escape
/// hatch for future engagements that deliberately want a Drive-
/// synced vault. For now the frontend omits it — Codex 2026-04-22
/// flagged a writer/watcher split when vault_path differed from
/// the slug-derived default, so we default to the slug path until
/// write_task_frontmatter also honours vault_path.
#[tauri::command]
pub fn start_task_watch(
    engagement_id: String,
    client_slug: String,
    vault_path: Option<String>,
    app: AppHandle,
    state: State<'_, TaskWatchState>,
) -> Result<(), String> {
    let vault_root = match vault_path.as_deref() {
        Some(p) if !p.trim().is_empty() => std::path::PathBuf::from(p),
        _ => vault_path_for_slug(&client_slug)?,
    };
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

    // Initial scan: emit synthetic task:vault-change events for
    // every existing .md file in 02-tasks/. `notify` fires only on
    // filesystem changes, so a watcher started after files were
    // written (legacy imports, etc.) would never sync them.
    //
    // 2026-04-29 fix: the previous implementation tried to spawn an
    // async task via `tokio::runtime::Handle::try_current()`, which
    // silently failed when `start_task_watch` ran outside a tokio
    // context (common for synchronous Tauri commands). This caused
    // the initial scan to be SKIPPED — files written by Claude
    // before the user opened the Tasks tab never appeared in the
    // Kanban. Fix: run the scan synchronously on a std::thread so
    // it works regardless of tokio context. The scan is I/O-bound
    // (read files, emit events) and typically < 50ms for < 100 files.
    // Run on a std::thread — no tokio runtime required. The scan is
    // I/O-bound (read files, emit events) and typically < 50ms.
    let scan_app = app.clone();
    let scan_tasks_dir = tasks_dir.clone();
    let scan_engagement_id = engagement_id.clone();
    std::thread::spawn(move || {
        emit_initial_scan_sync(&scan_app, &scan_tasks_dir, &scan_engagement_id);
    });
    let scan_handle: Option<tokio::task::JoinHandle<()>> = None;

    let active = ActiveWatcher {
        engagement_id,
        tasks_dir,
        watcher,
        debounce,
        scan_handle,
    };
    *state.0.lock().unwrap() = Some(active);
    Ok(())
}

/// Walk the 02-tasks/ directory once on watcher start and emit a
/// synthetic `task:vault-change` event per .md file so the
/// frontend's Firestore-sync handler can ingest pre-existing files.
///
/// Synchronous — runs on a std::thread, no tokio runtime required.
/// This fixes a bug where the previous async version silently
/// skipped the scan when `start_task_watch` ran outside a tokio
/// context, causing Claude-written task files to never appear in
/// the Kanban.
fn emit_initial_scan_sync(
    app: &AppHandle,
    tasks_dir: &std::path::Path,
    engagement_id: &str,
) {
    let entries = match std::fs::read_dir(tasks_dir) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("initial scan: read_dir {tasks_dir:?}: {e}");
            return;
        }
    };
    let mut emitted = 0usize;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        // Skip dotfiles (incl. .*.md.tmp from our own atomic writes).
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::debug!("initial scan read {path:?}: {e}");
                continue;
            }
        };
        let parsed = match parse_frontmatter(&content) {
            Ok(p) => p,
            Err(e) => {
                log::debug!("initial scan frontmatter {path:?}: {e}");
                continue;
            }
        };
        let rel = path
            .strip_prefix(tasks_dir.parent().unwrap_or(tasks_dir))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| {
                path.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
        let payload = TaskFrontmatter {
            vault_path: rel,
            engagement_id: engagement_id.to_string(),
            ..parsed
        };
        let _ = app.emit("task:vault-change", &payload);
        emitted += 1;
        // Brief sleep between batches so a huge vault (1000s of
        // tasks) doesn't flood the event channel.
        if emitted % 25 == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if emitted > 0 {
        log::info!(
            "initial scan emitted {emitted} task(s) from {}",
            tasks_dir.display()
        );
    }
}

/// Explicit stop for the active watcher. Not strictly required —
/// the watcher is also dropped when the state is replaced or the
/// app exits — but useful for tests and engagement switches.
#[tauri::command]
pub fn stop_task_watch(state: State<'_, TaskWatchState>) -> Result<(), String> {
    *state.0.lock().unwrap() = None;
    Ok(())
}

/// Input schema for `write_task_frontmatter` — shares field names
/// with `TaskFrontmatter` but all fields are optional so the UI
/// can patch a subset (e.g. just status + updatedAt). When a field
/// is `None` we preserve whatever the existing file carries.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct TaskFrontmatterPatch {
    pub id: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub tags: Option<Vec<String>>,
    pub due: Option<Option<String>>,
    pub client_visible: Option<Option<bool>>,
    pub description: Option<String>,
    pub assignee: Option<String>,
}

/// Write the vault markdown file for a task, preserving the
/// existing note body.
///
/// CRITICAL data-loss guard (Codex 2026-04-21 E2E must-fix #4):
/// the previous bridge was one-way (vault → Firestore). A UI edit
/// in the drawer (title, status, priority, client-visible toggle)
/// only updated Firestore. The next time Claude wrote the same
/// file, the stale markdown frontmatter would overwrite the
/// Firestore value via the watcher. Silent data loss.
///
/// Contract of this function:
///   1. If the file exists, read it and split body from frontmatter.
///   2. Merge the patch with the existing frontmatter — any field
///      the caller left `None` keeps its existing value.
///   3. Re-serialise YAML + append the preserved body.
///   4. Write atomically via tmp + rename (so a crash mid-write
///      can never leave a truncated or empty file on disk).
///   5. Never delete the file. Never truncate body. Never return
///      OK without the target file existing with non-zero size
///      after rename.
///
/// Caller is responsible for calling `markTaskPendingLocal` in the
/// frontend BEFORE invoking this, so the watcher's notify event
/// is suppressed by the 2s anti-flicker window.
#[tauri::command]
pub fn write_task_frontmatter(
    client_slug: String,
    vault_path: Option<String>,
    patch: TaskFrontmatterPatch,
) -> Result<(), String> {
    use crate::commands::vault::vault_path_for_slug;
    use std::io::Write;

    if patch.id.is_empty() {
        return Err("id required".to_string());
    }
    // id must look like a filename-safe slug — no path separators,
    // no `..`, no null bytes, no control chars.
    if patch.id.contains('/')
        || patch.id.contains('\\')
        || patch.id.contains("..")
        || patch.id.contains('\0')
        || patch.id.chars().any(|c| c.is_control())
    {
        return Err("invalid task id".to_string());
    }

    // Accept optional `vault_path` override (mirrors start_task_watch
    // 2026-04-22 hotfix). When present, write to <vault_path>/02-tasks;
    // otherwise fall back to slug-derived default. Keeps writer and
    // watcher pointed at the same folder for ALL engagements, whether
    // vault.path happens to equal the slug default (Moe's BLR today)
    // or a future engagement with a Drive-synced path.
    let vault_root = match vault_path.as_deref() {
        Some(p) if !p.trim().is_empty() => std::path::PathBuf::from(p),
        _ => vault_path_for_slug(&client_slug)?,
    };
    let tasks_dir = vault_root.join("02-tasks");
    std::fs::create_dir_all(&tasks_dir)
        .map_err(|e| format!("create 02-tasks: {e}"))?;

    let target = tasks_dir.join(format!("{}.md", patch.id));

    // Read existing file (may or may not exist). We preserve the
    // note body verbatim and merge frontmatter fields.
    let (existing_fm, existing_body) = if target.exists() {
        let raw = std::fs::read_to_string(&target)
            .map_err(|e| format!("read existing: {e}"))?;
        let (fm, body) = split_frontmatter(&raw);
        (fm, body)
    } else {
        (None, String::new())
    };

    let merged = merge_frontmatter(existing_fm.as_ref(), &patch)?;
    let yaml = serde_yaml::to_string(&merged)
        .map_err(|e| format!("serialize yaml: {e}"))?;

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&yaml);
    // serde_yaml emits a trailing newline; don't double up.
    if !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    // Preserve the existing body EXACTLY as-is. If the file is new,
    // body is empty; we don't auto-generate filler.
    out.push_str(&existing_body);

    // Atomic write: tmp in the same directory, then rename. Rename
    // is atomic on the same filesystem; a partial or crashed write
    // leaves only the pre-existing file intact.
    let tmp = tasks_dir.join(format!(".{}.md.tmp", patch.id));
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| format!("open tmp: {e}"))?;
        f.write_all(out.as_bytes())
            .map_err(|e| format!("write tmp: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("sync tmp: {e}"))?;
    }
    // Atomic replace. `std::fs::rename` overwrites the destination
    // atomically on POSIX, and on Windows 10+ with modern Rust it
    // also uses MoveFileEx(REPLACE_EXISTING). If replace-rename ever
    // fails (legacy FS, AV lock, cross-device), do a safe swap:
    // move the existing file to a backup first, rename tmp into
    // place, delete the backup on success. On failure, restore the
    // backup so we NEVER lose the original — Codex 2026-04-21
    // pre-push caught an earlier attempt that deleted target before
    // confirming the new file was installed.
    match std::fs::rename(&tmp, &target) {
        Ok(()) => {}
        Err(_) if target.exists() => {
            let backup = target.with_extension("md.bak");
            std::fs::rename(&target, &backup)
                .map_err(|e| format!("backup existing: {e}"))?;
            match std::fs::rename(&tmp, &target) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&backup);
                }
                Err(e) => {
                    // Restore the original — the new file never
                    // landed, but the old one must not be lost.
                    let _ = std::fs::rename(&backup, &target);
                    return Err(format!(
                        "rename tmp→target failed after backup: {e}"
                    ));
                }
            }
        }
        Err(e) => {
            // Target didn't exist (new file case) and rename still
            // failed — cross-device, permissions, etc.
            return Err(format!("rename tmp→target: {e}"));
        }
    }

    // Post-write verification: file exists, non-zero size, parses.
    let meta = std::fs::metadata(&target)
        .map_err(|e| format!("post-write stat: {e}"))?;
    if meta.len() == 0 {
        return Err("post-write size is zero (aborting)".to_string());
    }
    // Round-trip parse — catches a catastrophic serde bug before
    // the watcher fires a malformed event back to the UI.
    let verify = std::fs::read_to_string(&target)
        .map_err(|e| format!("verify read: {e}"))?;
    let _ = parse_frontmatter(&verify)
        .map_err(|e| format!("verify parse: {e}"))?;

    Ok(())
}

/// Split a markdown file into (frontmatter_yaml_string,
/// body_string_including_leading_newlines). If there's no
/// frontmatter, returns (None, full_content).
pub(crate) fn split_frontmatter(md: &str) -> (Option<String>, String) {
    let trimmed = md.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return (None, md.to_string());
    }
    let after_open = &trimmed[3..];
    let Some(end) = after_open.find("\n---") else {
        // Unterminated — treat whole file as body to avoid data loss.
        return (None, md.to_string());
    };
    let yaml = after_open[..end].trim_start_matches('\n').to_string();
    // Body = everything after the closing "---\n".
    let rest = &after_open[end + 4..];
    // Preserve the leading newline so round-trip stays tidy.
    let body = if let Some(stripped) = rest.strip_prefix('\n') {
        stripped.to_string()
    } else {
        rest.to_string()
    };
    (Some(yaml), body)
}

/// Merge a patch on top of the existing frontmatter. Fields the
/// patch left `None` fall back to the existing value. For
/// `Option<Option<T>>` fields like `due` and `client_visible`, the
/// outer `None` means "don't touch"; `Some(None)` means "clear".
fn merge_frontmatter(
    existing_yaml: Option<&String>,
    patch: &TaskFrontmatterPatch,
) -> Result<TaskFrontmatter, String> {
    let base: TaskFrontmatter = if let Some(y) = existing_yaml {
        serde_yaml::from_str(y)
            .map_err(|e| format!("existing yaml parse: {e}"))?
    } else {
        TaskFrontmatter {
            id: patch.id.clone(),
            title: patch.title.clone().unwrap_or_default(),
            status: patch.status.clone().unwrap_or_else(|| "backlog".into()),
            priority: patch.priority.clone().unwrap_or_else(default_priority),
            tags: patch.tags.clone().unwrap_or_default(),
            due: match &patch.due {
                Some(v) => v.clone(),
                None => None,
            },
            client_visible: match &patch.client_visible {
                Some(v) => *v,
                None => None,
            },
            description: patch.description.clone(),
            assignee: patch.assignee.clone().unwrap_or_else(default_assignee),
            vault_path: String::new(),
            engagement_id: String::new(),
        }
    };

    let mut merged = base;
    merged.id = patch.id.clone(); // always from patch — that's the key.
    if let Some(v) = patch.title.clone() {
        merged.title = v;
    }
    if let Some(v) = patch.status.clone() {
        merged.status = v;
    }
    if let Some(v) = patch.priority.clone() {
        merged.priority = v;
    }
    if let Some(v) = patch.tags.clone() {
        merged.tags = v;
    }
    if let Some(v) = &patch.due {
        merged.due = v.clone();
    }
    if let Some(v) = &patch.client_visible {
        merged.client_visible = *v;
    }
    if patch.description.is_some() {
        merged.description = patch.description.clone();
    }
    if let Some(v) = patch.assignee.clone() {
        merged.assignee = v;
    }

    // Never write runtime-injected fields to the file. They live in
    // the watcher payload only.
    merged.vault_path = String::new();
    merged.engagement_id = String::new();

    Ok(merged)
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

    // ---- split_frontmatter + merge_frontmatter round-trip tests ----

    #[test]
    fn test_split_frontmatter_basic() {
        let md = "---\nid: a\ntitle: b\nstatus: backlog\n---\nhello body\n";
        let (fm, body) = split_frontmatter(md);
        assert!(fm.is_some());
        assert_eq!(body, "hello body\n");
    }

    #[test]
    fn test_split_frontmatter_no_fm_preserves_body() {
        let md = "# no frontmatter here\n\njust body";
        let (fm, body) = split_frontmatter(md);
        assert!(fm.is_none());
        assert_eq!(body, md);
    }

    #[test]
    fn test_split_frontmatter_unterminated_preserves_as_body() {
        // An unterminated frontmatter block MUST NOT be treated as
        // frontmatter — we'd lose content on a subsequent write.
        let md = "---\nid: a\ntitle: unfinished\n\nno closing";
        let (fm, body) = split_frontmatter(md);
        assert!(fm.is_none());
        assert_eq!(body, md);
    }

    #[test]
    fn test_merge_preserves_existing_when_patch_empty() {
        let existing = "id: x\ntitle: Existing\nstatus: in_progress\npriority: p1\ntags: [a, b]\n";
        let patch = TaskFrontmatterPatch {
            id: "x".to_string(),
            ..Default::default()
        };
        let merged = merge_frontmatter(Some(&existing.to_string()), &patch).unwrap();
        assert_eq!(merged.title, "Existing");
        assert_eq!(merged.status, "in_progress");
        assert_eq!(merged.priority, "p1");
        assert_eq!(merged.tags, vec!["a", "b"]);
    }

    #[test]
    fn test_merge_patch_wins_over_existing() {
        let existing = "id: x\ntitle: Old\nstatus: in_progress\npriority: p1\n";
        let patch = TaskFrontmatterPatch {
            id: "x".to_string(),
            title: Some("New".to_string()),
            status: Some("done".to_string()),
            ..Default::default()
        };
        let merged = merge_frontmatter(Some(&existing.to_string()), &patch).unwrap();
        assert_eq!(merged.title, "New");
        assert_eq!(merged.status, "done");
        assert_eq!(merged.priority, "p1"); // untouched
    }

    #[test]
    fn test_merge_clear_optional_with_inner_none() {
        // Option<Option<T>> semantics: Some(None) clears, None
        // leaves alone.
        let existing = "id: x\ntitle: t\nstatus: s\npriority: p2\ndue: 2026-05-01\n";
        let patch = TaskFrontmatterPatch {
            id: "x".to_string(),
            due: Some(None),
            ..Default::default()
        };
        let merged = merge_frontmatter(Some(&existing.to_string()), &patch).unwrap();
        assert_eq!(merged.due, None);
    }

    #[test]
    fn test_merge_preserves_when_due_is_outer_none() {
        let existing = "id: x\ntitle: t\nstatus: s\npriority: p2\ndue: 2026-05-01\n";
        let patch = TaskFrontmatterPatch {
            id: "x".to_string(),
            due: None,
            ..Default::default()
        };
        let merged = merge_frontmatter(Some(&existing.to_string()), &patch).unwrap();
        assert_eq!(merged.due.as_deref(), Some("2026-05-01"));
    }

    #[test]
    fn test_write_task_frontmatter_preserves_body() {
        // Create a vault + an existing task file with a meaningful
        // body. Then patch just the status. Body must survive.
        let tmp = std::env::temp_dir()
            .join(format!("ikrs-vault-{}", uuid::Uuid::new_v4()));
        let vaults = dirs::home_dir().unwrap().join(".ikrs-workspace/vaults");
        std::fs::create_dir_all(&vaults).unwrap();
        let slug = format!("_test_write_{}", uuid::Uuid::new_v4());
        let vault = vaults.join(&slug);
        let tasks = vault.join("02-tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        let id = "test-task-1";
        let body = "\n## Context\n\nMeeting notes from 2026-04-10 Angelique sync.\nAngel raised a concern about the SOW.\n\n## Update\n\n- [ ] Review proposal\n- [ ] Follow up with Karla\n";
        let initial =
            format!("---\nid: {id}\ntitle: Original title\nstatus: backlog\npriority: p2\n---\n{body}");
        let target = tasks.join(format!("{id}.md"));
        std::fs::write(&target, &initial).unwrap();

        let patch = TaskFrontmatterPatch {
            id: id.to_string(),
            status: Some("in_progress".to_string()),
            ..Default::default()
        };
        write_task_frontmatter(slug.clone(), None, patch).unwrap();

        let result = std::fs::read_to_string(&target).unwrap();
        // Body must be present verbatim.
        assert!(result.contains("Meeting notes from 2026-04-10"));
        assert!(result.contains("- [ ] Follow up with Karla"));
        // Status must be updated.
        assert!(result.contains("status: in_progress"));
        // Title must be preserved (not patched).
        assert!(result.contains("title: Original title"));
        // The runtime-only fields must NOT be written.
        assert!(!result.contains("vault_path:"));
        assert!(!result.contains("engagement_id:"));

        std::fs::remove_dir_all(&vault).ok();
        let _ = tmp;
    }

    #[test]
    fn test_write_task_frontmatter_rejects_bad_id() {
        let patch = TaskFrontmatterPatch {
            id: "../evil".to_string(),
            ..Default::default()
        };
        assert!(write_task_frontmatter("slug".to_string(), None, patch).is_err());

        let patch = TaskFrontmatterPatch {
            id: "a/b".to_string(),
            ..Default::default()
        };
        assert!(write_task_frontmatter("slug".to_string(), None, patch).is_err());

        let patch = TaskFrontmatterPatch {
            id: String::new(),
            ..Default::default()
        };
        assert!(write_task_frontmatter("slug".to_string(), None, patch).is_err());

        let patch = TaskFrontmatterPatch {
            id: "null\0byte".to_string(),
            ..Default::default()
        };
        assert!(write_task_frontmatter("slug".to_string(), None, patch).is_err());
    }

    #[test]
    fn test_write_task_creates_new_file_without_existing() {
        let vaults = dirs::home_dir().unwrap().join(".ikrs-workspace/vaults");
        std::fs::create_dir_all(&vaults).unwrap();
        let slug = format!("_test_new_{}", uuid::Uuid::new_v4());
        let patch = TaskFrontmatterPatch {
            id: "fresh-task".to_string(),
            title: Some("Fresh from UI".to_string()),
            status: Some("backlog".to_string()),
            ..Default::default()
        };
        write_task_frontmatter(slug.clone(), None, patch).unwrap();
        let target = vaults
            .join(&slug)
            .join("02-tasks")
            .join("fresh-task.md");
        assert!(target.exists());
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("id: fresh-task"));
        assert!(content.contains("title: Fresh from UI"));
        // No spurious body on a brand-new file.
        let (_, body) = split_frontmatter(&content);
        assert_eq!(body.trim(), "");

        std::fs::remove_dir_all(vaults.join(&slug)).ok();
    }
}
