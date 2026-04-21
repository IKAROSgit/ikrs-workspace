use std::fs;
use std::path::PathBuf;

use serde::Serialize;

fn vault_base() -> PathBuf {
    dirs::home_dir()
        .expect("No home directory")
        .join(".ikrs-workspace")
        .join("vaults")
}

/// Resolve the vault path for a client slug, with basic slug
/// validation (no path separators, no traversal, no empty). Shared
/// by vault / notes_sync / future direct-filesystem commands.
pub fn vault_path_for_slug(slug: &str) -> Result<PathBuf, String> {
    if slug.is_empty()
        || slug.contains('/')
        || slug.contains('\\')
        || slug.contains("..")
    {
        return Err(format!("invalid client slug: {slug:?}"));
    }
    Ok(vault_base().join(slug))
}

#[tauri::command]
pub async fn create_vault(client_slug: String) -> Result<String, String> {
    let vault_path = vault_base().join(&client_slug);
    if vault_path.exists() {
        return Ok(vault_path.to_string_lossy().to_string());
    }

    let dirs_to_create = [
        ".obsidian",
        "00-inbox",
        "01-meetings",
        "02-tasks",
        "03-deliverables",
        "04-reference",
        "_templates",
    ];

    for dir in &dirs_to_create {
        fs::create_dir_all(vault_path.join(dir))
            .map_err(|e| format!("Failed to create vault dir {dir}: {e}"))?;
    }

    let templates = [
        ("_templates/meeting-note.md", "# Meeting: {{title}}\n\n**Date:** {{date}}\n**Attendees:** \n\n## Agenda\n\n## Notes\n\n## Action Items\n"),
        ("_templates/task-note.md", "# Task: {{title}}\n\n**Status:** \n**Priority:** \n\n## Context\n\n## Progress\n\n## Blockers\n"),
        ("_templates/daily-note.md", "# {{date}}\n\n## Focus\n\n## Log\n\n## Reflections\n"),
    ];

    for (path, content) in &templates {
        fs::write(vault_path.join(path), content)
            .map_err(|e| format!("Failed to write {path}: {e}"))?;
    }

    let readme = format!(
        "# {} — Engagement Vault\n\nCreated by IKAROS Workspace.\n",
        client_slug
    );
    fs::write(vault_path.join("README.md"), readme)
        .map_err(|e| format!("Failed to write README: {e}"))?;

    fs::write(
        vault_path.join(".obsidian/app.json"),
        r#"{"theme":"obsidian"}"#,
    )
    .map_err(|e| format!("Failed to write obsidian config: {e}"))?;

    Ok(vault_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn archive_vault(client_slug: String) -> Result<String, String> {
    let vault_path = vault_base().join(&client_slug);
    if !vault_path.exists() {
        return Err(format!("Vault not found: {client_slug}"));
    }

    let archive_dir = dirs::home_dir()
        .expect("No home directory")
        .join(".ikrs-workspace")
        .join("archive");
    fs::create_dir_all(&archive_dir).map_err(|e| e.to_string())?;

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let archive_name = format!("{client_slug}-{date}.tar.gz");
    let archive_path = archive_dir.join(&archive_name);

    let tar_gz = fs::File::create(&archive_path).map_err(|e| e.to_string())?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(&client_slug, &vault_path)
        .map_err(|e| format!("Failed to archive: {e}"))?;
    tar.finish().map_err(|e| e.to_string())?;

    fs::remove_dir_all(&vault_path).map_err(|e| e.to_string())?;

    Ok(archive_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn restore_vault(archive_path: String) -> Result<String, String> {
    let archive = std::path::Path::new(&archive_path);
    if !archive.exists() {
        return Err(format!("Archive not found: {archive_path}"));
    }

    let tar_gz = fs::File::open(archive).map_err(|e| e.to_string())?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    let mut tar = tar::Archive::new(dec);
    tar.unpack(vault_base())
        .map_err(|e| format!("Failed to restore: {e}"))?;

    Ok("Vault restored".to_string())
}

#[tauri::command]
pub async fn delete_vault(client_slug: String) -> Result<(), String> {
    let vault_path = vault_base().join(&client_slug);
    if vault_path.exists() {
        fs::remove_dir_all(&vault_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct RecentNote {
    /// Path relative to vault root (e.g. `01-meetings/2026-04-21-blr-sync.md`).
    pub relative_path: String,
    /// First heading in the file (the `#` or `##` line), or the filename
    /// stem if no heading. Truncated to 120 chars.
    pub title: String,
    /// Unix millis of the file's last modification.
    pub modified_at: u64,
    /// File size in bytes. Useful for showing "small note" / "long doc".
    pub size_bytes: u64,
}

/// Lists the N most-recently-modified markdown files in a vault, with
/// a short preview derived from the first heading. Used by the
/// session-boot briefing to give Claude "recent activity" context
/// without having to stream every file into the prompt.
///
/// Skips:
///   - Anything starting with `.` (dotfiles, `.obsidian/`, `.trash/`)
///   - `_memory/` — consultant-internal memory, not "recent activity"
///   - `_templates/` — inert scaffolding
///   - `node_modules/` if one ever lands inside a vault
///
/// `client_slug` is the vault folder name. Validation via
/// `vault_path_for_slug` rejects traversal / separators. `limit` is
/// capped at 25 to keep the briefing payload bounded.
#[tauri::command]
pub async fn list_recent_vault_notes(
    client_slug: String,
    limit: Option<usize>,
) -> Result<Vec<RecentNote>, String> {
    let vault = vault_path_for_slug(&client_slug)?;
    if !vault.exists() {
        return Ok(Vec::new());
    }
    let cap = limit.unwrap_or(5).min(25).max(1);

    let mut entries: Vec<(std::path::PathBuf, std::time::SystemTime, u64)> = Vec::new();
    walk_markdown(&vault, &vault, &mut entries)
        .map_err(|e| format!("walk failed: {e}"))?;

    entries.sort_by(|a, b| b.1.cmp(&a.1)); // newest first
    entries.truncate(cap);

    let notes = entries
        .into_iter()
        .map(|(path, mtime, size)| {
            let rel = path
                .strip_prefix(&vault)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let title = first_heading_or_stem(&path).unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| rel.clone())
            });
            let modified_at = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            RecentNote {
                relative_path: rel,
                title,
                modified_at,
                size_bytes: size,
            }
        })
        .collect();

    Ok(notes)
}

fn walk_markdown(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(std::path::PathBuf, std::time::SystemTime, u64)>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden + reserved folders.
        if name_str.starts_with('.')
            || name_str == "_memory"
            || name_str == "_templates"
            || name_str == "node_modules"
        {
            continue;
        }

        let meta = entry.metadata()?;
        if meta.is_dir() {
            walk_markdown(root, &path, out)?;
        } else if meta.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        {
            if let Ok(mtime) = meta.modified() {
                out.push((path, mtime, meta.len()));
            }
        }
    }
    Ok(())
}

fn first_heading_or_stem(path: &std::path::Path) -> Option<String> {
    // Read up to 8KB — headings are almost always in the first few
    // lines, and we don't want to slurp multi-MB notes for a preview.
    use std::io::Read;
    let mut f = fs::File::open(path).ok()?;
    let mut buf = [0u8; 8192];
    let n = f.read(&mut buf).ok()?;
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    // Skip YAML frontmatter if present so the heading below it wins.
    let body = if text.starts_with("---\n") {
        if let Some(end) = text[4..].find("\n---") {
            &text[4 + end + 4..]
        } else {
            text
        }
    } else {
        text
    };
    for line in body.lines().take(40) {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('#') {
            // Strip any additional `#` (for ##, ###, etc.) and whitespace.
            let title = rest.trim_start_matches('#').trim();
            if !title.is_empty() {
                let mut t = title.to_string();
                if t.len() > 120 {
                    t.truncate(120);
                    t.push('…');
                }
                return Some(t);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_vault() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        (tempfile::TempDir::new().unwrap_or_else(|_| panic!("tempdir")), vault)
    }

    fn write_note(dir: &std::path::Path, name: &str, content: &str) {
        fs::create_dir_all(dir.parent().unwrap_or(dir)).unwrap();
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn first_heading_prefers_h1_over_h2() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.md");
        std::fs::write(&p, "# Title\n## Sub\n").unwrap();
        assert_eq!(first_heading_or_stem(&p), Some("Title".to_string()));
    }

    #[test]
    fn first_heading_skips_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("b.md");
        std::fs::write(
            &p,
            "---\nid: foo\ntitle: ignored\n---\n\n# Real title\n",
        )
        .unwrap();
        assert_eq!(first_heading_or_stem(&p), Some("Real title".to_string()));
    }

    #[test]
    fn first_heading_truncates_very_long() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("c.md");
        let long = "a".repeat(200);
        std::fs::write(&p, format!("# {}\n", long)).unwrap();
        let got = first_heading_or_stem(&p).unwrap();
        // 120 chars of 'a' + '…'. UTF-8 '…' is 3 bytes, so len() is 123.
        // char_count is 121.
        assert_eq!(got.chars().count(), 121);
        assert!(got.ends_with('…'));
    }

    #[test]
    fn walk_markdown_skips_hidden_and_reserved() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join("_memory")).unwrap();
        fs::create_dir_all(root.join("_templates")).unwrap();
        fs::create_dir_all(root.join("01-meetings")).unwrap();
        write_note(&root.join(".obsidian"), "config.md", "# hidden\n");
        write_note(&root.join("_memory"), "principles.md", "# skipme\n");
        write_note(&root.join("_templates"), "meet.md", "# template\n");
        write_note(&root.join("01-meetings"), "real.md", "# real\n");
        write_note(root, "top.md", "# top\n");

        let mut out = Vec::new();
        walk_markdown(root, root, &mut out).unwrap();
        let names: Vec<String> = out
            .iter()
            .map(|(p, _, _)| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(names.iter().any(|n| n == "top.md"));
        assert!(names.iter().any(|n| n.ends_with("real.md")));
        assert!(!names.iter().any(|n| n.contains(".obsidian")));
        assert!(!names.iter().any(|n| n.contains("_memory")));
        assert!(!names.iter().any(|n| n.contains("_templates")));
    }

    // Silence unused-function warning — tmp_vault is here for future
    // integration-level tests that need a fully-seeded vault.
    #[allow(dead_code)]
    fn _use_tmp_vault() {
        let (_, _) = tmp_vault();
    }
}
