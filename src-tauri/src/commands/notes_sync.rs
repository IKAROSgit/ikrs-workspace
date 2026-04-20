//! Notes sync — direct read of the engagement's vault filesystem.
//!
//! No network, no MCP: the vault lives at
//! `~/.ikrs-workspace/vaults/<slug>/` and contains the consultant's
//! markdown artifacts (meeting notes, planning docs, briefs, etc).
//! Bypassing the obsidian MCP for the Notes view makes refresh
//! instant (single stat walk) and works offline.
//!
//! Scope:
//!   - Walk vault root, one level deep per directory
//!   - Filter to `.md` files (hidden dotfiles excluded)
//!   - Return ordered by mtime desc
//!   - No content read on list (too much memory); content fetched
//!     on a per-file open via `read_note_content`
//!
//! Security:
//!   - Walk bounded to the vault path; path-traversal rejected
//!   - Symlinks are NOT followed (prevents escape to host fs)

use crate::commands::vault::vault_path_for_slug;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct VaultFile {
    pub path: String,
    pub name: String,
    pub rel_path: String,
    pub is_directory: bool,
    pub size_bytes: u64,
    pub modified_unix: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum NotesResult {
    Ok { files: Vec<VaultFile> },
    NoVault,
    Other { message: String },
}

#[tauri::command]
pub async fn list_vault_notes(client_slug: String) -> Result<NotesResult, String> {
    let vault_root = match vault_path_for_slug(&client_slug) {
        Ok(p) => p,
        Err(e) => {
            log::info!("list_vault_notes: bad slug — {e}");
            return Ok(NotesResult::NoVault);
        }
    };

    if !vault_root.exists() {
        return Ok(NotesResult::NoVault);
    }
    // Require the `.skill-version` marker — proves the directory
    // was scaffolded by our app (not some arbitrary folder under
    // ~/.ikrs-workspace/vaults/). Defense-in-depth against
    // malformed client_slug passing vault_path_for_slug shape check.
    if !vault_root.join(".skill-version").exists() {
        log::info!("list_vault_notes: no .skill-version marker at {vault_root:?}");
        return Ok(NotesResult::NoVault);
    }

    let files = tokio::task::spawn_blocking(move || walk_vault(&vault_root))
        .await
        .map_err(|e| format!("notes walk join: {e}"))?;

    match files {
        Ok(mut list) => {
            list.sort_by(|a, b| b.modified_unix.cmp(&a.modified_unix));
            Ok(NotesResult::Ok { files: list })
        }
        Err(e) => Ok(NotesResult::Other { message: e }),
    }
}

#[tauri::command]
pub async fn read_note_content(
    client_slug: String,
    rel_path: String,
) -> Result<String, String> {
    let vault_root = vault_path_for_slug(&client_slug)?;

    // ACL gate: only scaffolded engagement vaults are readable.
    // `.skill-version` is written by scaffold_engagement_skills at
    // vault creation time, so its presence proves this directory
    // was provisioned by our app as an engagement workspace. An
    // attacker passing an arbitrary `client_slug` that happens to
    // match some other directory under ~/.ikrs-workspace/vaults/
    // still fails closed unless that directory was formally
    // scaffolded.
    if !vault_root.join(".skill-version").exists() {
        return Err("not a scaffolded engagement vault".to_string());
    }

    let target = sanitize_join(&vault_root, &rel_path)?;

    // Symmetric file-size cap to prevent a malicious/renamed binary
    // inside the vault from being read into memory. Notes are
    // markdown; 10 MB is plenty.
    const MAX_READ_BYTES: u64 = 10 * 1024 * 1024;
    if let Ok(meta) = std::fs::metadata(&target) {
        if meta.len() > MAX_READ_BYTES {
            return Err(format!(
                "file too large ({} bytes > {})",
                meta.len(),
                MAX_READ_BYTES
            ));
        }
    }

    tokio::task::spawn_blocking(move || std::fs::read_to_string(&target))
        .await
        .map_err(|e| format!("read join: {e}"))?
        .map_err(|e| format!("read: {e}"))
}

/// Walks the vault and returns all directories + `.md` files.
/// Bounded depth = 5 so we don't recurse forever on a pathological
/// symlink loop (symlinks are never followed, but depth is a
/// belt-and-braces guard).
fn walk_vault(root: &std::path::Path) -> Result<Vec<VaultFile>, String> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, u8)> = vec![(root.to_path_buf(), 0)];
    const MAX_DEPTH: u8 = 5;

    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            if name.starts_with('.') {
                continue; // .claude, .skill-version, .mcp-config.json, etc.
            }
            let path = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Never follow symlinks.
            if meta.file_type().is_symlink() {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| name.clone());
            let modified_unix = meta
                .modified()
                .ok()
                .and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs() as i64)
                })
                .unwrap_or(0);

            if meta.is_dir() {
                out.push(VaultFile {
                    path: path.to_string_lossy().to_string(),
                    name,
                    rel_path: rel,
                    is_directory: true,
                    size_bytes: 0,
                    modified_unix,
                });
                stack.push((path, depth + 1));
            } else if meta.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("md")
            {
                out.push(VaultFile {
                    path: path.to_string_lossy().to_string(),
                    name,
                    rel_path: rel,
                    is_directory: false,
                    size_bytes: meta.len(),
                    modified_unix,
                });
            }
        }
    }
    Ok(out)
}

fn sanitize_join(
    root: &std::path::Path,
    rel: &str,
) -> Result<std::path::PathBuf, String> {
    // Reject absolute paths, `..` traversal, backslash tricks,
    // null bytes, and any control characters. Belt-and-braces
    // additions per Codex 2026-04-20 review.
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains("..")
        || rel.contains('\\')
        || rel.contains('\0')
        || rel.chars().any(|c| c.is_control())
    {
        return Err("invalid relative path".to_string());
    }
    let joined = root.join(rel);

    // symlink_metadata sees the link itself (not its target), so we
    // can fail closed BEFORE canonicalize touches the target.
    // A well-placed symlink to /etc/passwd inside the vault would
    // otherwise canonicalize-through and possibly pass prefix check
    // if the host filesystem has odd casing — defense in depth.
    if let Ok(lm) = std::fs::symlink_metadata(&joined) {
        if lm.file_type().is_symlink() {
            return Err("path is a symlink (not allowed)".to_string());
        }
    }

    // Canonicalize both sides and enforce containment.
    let canon_root = std::fs::canonicalize(root)
        .map_err(|e| format!("canonicalize root: {e}"))?;
    let canon_target = std::fs::canonicalize(&joined)
        .map_err(|e| format!("canonicalize target: {e}"))?;
    if !canon_target.starts_with(&canon_root) {
        return Err("path escape".to_string());
    }
    Ok(canon_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("ikrs-notes-{}-{}", label, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_walk_vault_filters_md_and_skips_hidden() {
        let root = make_root("walk");
        std::fs::write(root.join("note.md"), "hello").unwrap();
        std::fs::write(root.join("skip.txt"), "nope").unwrap();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(root.join(".claude/settings.local.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("planning")).unwrap();
        std::fs::write(root.join("planning/tracker.md"), "plan").unwrap();

        let files = walk_vault(&root).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.rel_path.clone()).collect();
        assert!(paths.iter().any(|p| p == "note.md"));
        assert!(paths.iter().any(|p| p.ends_with("planning/tracker.md") || p == "planning/tracker.md"));
        assert!(paths.iter().any(|p| p == "planning"));
        // .claude hidden dir is excluded
        assert!(!paths.iter().any(|p| p.contains(".claude")));
        // .txt is excluded
        assert!(!paths.iter().any(|p| p.ends_with("skip.txt")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_sanitize_join_rejects_traversal() {
        let root = make_root("san");
        std::fs::write(root.join("ok.md"), "ok").unwrap();

        assert!(sanitize_join(&root, "../etc/passwd").is_err());
        assert!(sanitize_join(&root, "/etc/passwd").is_err());
        assert!(sanitize_join(&root, "sub\\..\\..\\x").is_err());
        assert!(sanitize_join(&root, "").is_err());

        // Legit relative path works.
        assert!(sanitize_join(&root, "ok.md").is_ok());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_sanitize_join_rejects_null_byte_and_control_chars() {
        let root = make_root("null");
        assert!(sanitize_join(&root, "evil\0.md").is_err());
        assert!(sanitize_join(&root, "tab\there").is_err());
        assert!(sanitize_join(&root, "bell\x07").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn test_walk_vault_skips_symlinks() {
        // Symlink inside the vault pointing outside MUST not be
        // followed by walk. Otherwise a malicious/ill-configured
        // symlink could expose host-fs content via the Notes view.
        use std::os::unix::fs::symlink;
        let root = make_root("symlink");
        let outside = std::env::temp_dir().join(format!(
            "ikrs-notes-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.md"), "secret content").unwrap();
        symlink(&outside, root.join("escape")).unwrap();

        let files = walk_vault(&root).unwrap();
        assert!(
            !files.iter().any(|f| f.rel_path.contains("escape")
                || f.path.contains("outside")),
            "walk should skip symlinks; got {files:?}"
        );
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[cfg(unix)]
    #[test]
    fn test_sanitize_join_rejects_symlinks_via_symlink_metadata() {
        // Belt-and-braces for symlink detection: the symlink check
        // must fire BEFORE canonicalize reaches the target, even on
        // filesystems where canonicalize succeeds through the link.
        use std::os::unix::fs::symlink;
        let root = make_root("symjoin");
        let outside = std::env::temp_dir().join(format!(
            "ikrs-join-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("real.md"), "content").unwrap();
        symlink(outside.join("real.md"), root.join("linked.md")).unwrap();

        let result = sanitize_join(&root, "linked.md");
        assert!(
            result.is_err(),
            "sanitize_join must reject a symlink path; got {result:?}"
        );
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn test_notes_result_serde() {
        for (v, tag) in [
            (
                NotesResult::Ok {
                    files: vec![],
                },
                "ok",
            ),
            (NotesResult::NoVault, "no_vault"),
            (
                NotesResult::Other {
                    message: "x".to_string(),
                },
                "other",
            ),
        ] {
            let j = serde_json::to_string(&v).unwrap();
            assert!(j.contains(&format!("\"status\":\"{tag}\"")));
        }
    }
}
