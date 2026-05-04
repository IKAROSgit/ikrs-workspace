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
    let vault_path = vault_path_for_slug(&client_slug)?;
    if vault_path.exists() {
        // Even on existing vaults, run the idempotent scaffolder so
        // older vaults pick up new folders/templates added in later
        // releases (Phase 4C daily-notes/, 05-decisions/, etc.).
        scaffold_vault_idempotent(&vault_path, &client_slug)?;
        return Ok(vault_path.to_string_lossy().to_string());
    }
    scaffold_vault_idempotent(&vault_path, &client_slug)?;
    Ok(vault_path.to_string_lossy().to_string())
}

/// Idempotent vault scaffolder — creates only what's missing,
/// touches nothing that exists. Safe to call on session spawn so
/// existing (pre-Phase-4C) engagements pick up the new structure
/// without a migration step.
///
/// Design (M3 Phase 4C, 2026-04-21):
///   - Additive only. Never renames, never overwrites.
///   - Folders that already exist from the legacy `create_vault`
///     list (`00-inbox`, `01-meetings`, `02-tasks`, etc.) are left
///     alone; new ones (`05-decisions`, `06-briefs`, `daily-notes`,
///     `_memory`) are created.
///   - Templates written only when the target file doesn't exist.
///   - `CLAUDE.md` and `README.md` written only when missing so
///     consultants who've customised their vault prompt/readme
///     don't lose the edits.
pub fn scaffold_vault_idempotent(
    vault_path: &std::path::Path,
    client_slug: &str,
) -> Result<(), String> {
    let dirs_to_create = [
        ".obsidian",
        "00-inbox",
        "01-meetings",
        "02-tasks",
        "03-deliverables",
        "04-reference",
        "05-decisions",
        "06-briefs",
        "daily-notes",
        "_memory",
        "_memory/archive",
        "_templates",
    ];
    for dir in &dirs_to_create {
        let full = vault_path.join(dir);
        if !full.exists() {
            fs::create_dir_all(&full)
                .map_err(|e| format!("Failed to create vault dir {dir}: {e}"))?;
        }
    }

    // Templates: create only if absent so edits survive.
    let templates: &[(&str, &str)] = &[
        (
            "_templates/meeting-note.md",
            "---\ntype: meeting\ndate: {{date}}\nattendees: []\n---\n\n# {{title}}\n\n## Agenda\n\n## Notes\n\n## Decisions\n\n## Action items\n",
        ),
        (
            "_templates/task-note.md",
            "# Task: {{title}}\n\n**Status:** \n**Priority:** \n\n## Context\n\n## Progress\n\n## Blockers\n",
        ),
        (
            "_templates/daily-note.md",
            "# {{date}}\n\n## What's on today\n\n## Decisions\n\n## Log\n\n## Tomorrow\n",
        ),
        (
            "_templates/decision-note.md",
            "---\ntype: decision\ndate: {{date}}\nstatus: active\n---\n\n# {{title}}\n\n## Context\n\n## Options considered\n\n## Decision\n\n## Consequences\n\n## Follow-ups\n",
        ),
        (
            "_templates/brief-note.md",
            "---\ntype: brief\ndate: {{date}}\n---\n\n# {{title}}\n\n## Ask\n\n## Output\n",
        ),
    ];
    for (rel, content) in templates {
        let p = vault_path.join(rel);
        if !p.exists() {
            fs::write(&p, content)
                .map_err(|e| format!("Failed to write {rel}: {e}"))?;
        }
    }

    // Engagement-level CLAUDE.md — gives Claude conventions so it
    // saves meetings/decisions/briefs in the right places without
    // being told per session. Only written if absent — consultant
    // edits persist.
    let claude_md_path = vault_path.join("CLAUDE.md");
    if !claude_md_path.exists() {
        fs::write(&claude_md_path, engagement_claude_md(client_slug))
            .map_err(|e| format!("Failed to write CLAUDE.md: {e}"))?;
    }

    let readme_path = vault_path.join("README.md");
    if !readme_path.exists() {
        let readme = format!(
            "# {client_slug} — Engagement Vault\n\n\
             Created by IKAROS Workspace. Structure:\n\n\
             - `00-inbox/` — catch-all, sort later\n\
             - `01-meetings/` — meeting notes (YYYY-MM-DD-slug.md)\n\
             - `02-tasks/` — task files (managed by the app's Kanban)\n\
             - `03-deliverables/` — client-facing outputs\n\
             - `04-reference/` — research, docs, context\n\
             - `05-decisions/` — strategic / architectural decisions (NNN-slug.md)\n\
             - `06-briefs/` — quick prompt-to-artifact outputs\n\
             - `daily-notes/` — auto-created daily log (YYYY-MM-DD.md)\n\
             - `_memory/` — Claude's evolving memory (read at boot, written at session end)\n\
             - `_templates/` — note templates\n\n\
             See `CLAUDE.md` for the conventions Claude follows in this vault.\n"
        );
        fs::write(&readme_path, readme)
            .map_err(|e| format!("Failed to write README: {e}"))?;
    }

    let obsidian_cfg = vault_path.join(".obsidian/app.json");
    if !obsidian_cfg.exists() {
        fs::write(&obsidian_cfg, r#"{"theme":"obsidian"}"#)
            .map_err(|e| format!("Failed to write obsidian config: {e}"))?;
    }

    Ok(())
}

fn engagement_claude_md(client_slug: &str) -> String {
    format!(
        "# CLAUDE.md — engagement conventions for {client_slug}\n\n\
         This vault is managed by IKAROS Workspace. Follow these\n\
         conventions so files land where they belong and the\n\
         consultant can find them later.\n\n\
         ## Folder roles\n\n\
         - `00-inbox/` — catch-all for loose notes. Sort later.\n\
         - `01-meetings/YYYY-MM-DD-<slug>.md` — meeting notes. Use\n\
           the template in `_templates/meeting-note.md`.\n\
         - `02-tasks/<id>.md` — the app's Kanban is backed by files\n\
           in this folder. See \"Creating tasks\" below for the\n\
           required frontmatter. Do NOT manually edit a task file's\n\
           `id` field — you'll break the Kanban sync.\n\
         - `03-deliverables/` — client-facing outputs (proposals,\n\
           docs, reports).\n\
         - `04-reference/` — background research, client material,\n\
           context docs.\n\
         - `05-decisions/NNN-<slug>.md` — strategic or architectural\n\
           decisions. NNN is the next sequential integer\n\
           (zero-padded to 3 digits). Use the template in\n\
           `_templates/decision-note.md`.\n\
         - `06-briefs/YYYY-MM-DD-<slug>.md` — quick prompt-to-artifact\n\
           outputs: a draft email, a one-pager, a rough pitch.\n\
         - `daily-notes/YYYY-MM-DD.md` — the consultant's daily log.\n\
           The app auto-creates today's file at session boot; you\n\
           can append to it during the day.\n\
         - `_memory/` — your evolving memory for this engagement.\n\
           Read at session boot, written at session end. Contains\n\
           `principles.md` (how the consultant works), `lessons.md`\n\
           (gotchas), `relationships.md` (who's who on the client\n\
           side), `context.md` (current engagement state).\n\n\
         ## Creating tasks that show up in the Kanban\n\n\
         The app's Kanban view is wired to `02-tasks/`. When you\n\
         create a markdown file there with the right YAML frontmatter,\n\
         a filesystem watcher picks it up and the task appears on\n\
         the Kanban within a second or two. This is the ONLY way to\n\
         create app-level tasks from Claude — your internal TodoWrite\n\
         tool is separate (it only updates your own in-session task\n\
         panel, not the Kanban).\n\n\
         ### Required format\n\n\
         File path: `02-tasks/<id>.md` where `<id>` is a short unique\n\
         slug (e.g. `t-2026-04-21-procure-av-stack`).\n\n\
         ```markdown\n\
         ---\n\
         id: t-2026-04-21-procure-av-stack\n\
         title: Procure AV stack for Activate deck\n\
         status: in_progress\n\
         priority: p1\n\
         tags: [vendor, urgent]\n\
         due: 2026-04-28\n\
         client_visible: true\n\
         description: Confirm quotes from 2 AV vendors by Friday.\n\
         assignee: consultant\n\
         ---\n\n\
         Optional long-form notes go below the frontmatter. They\n\
         show up in the task drawer's notes timeline.\n\
         ```\n\n\
         ### Field reference\n\n\
         - `id` — unique, stable, lowercase-hyphenated. Claude should\n\
           NEVER reuse an id across tasks. Pick a descriptive slug,\n\
           not a UUID — it's easier for the consultant to read.\n\
         - `title` — short, imperative, 60 chars or under.\n\
         - `status` — one of: `backlog`, `in_progress`,\n\
           `awaiting_client`, `blocked`, `in_review`, `done`.\n\
         - `priority` — `p1` (urgent/blocker) · `p2` (default) · `p3`\n\
           (nice to have).\n\
         - `tags` — optional list of short strings.\n\
         - `due` — optional `YYYY-MM-DD`. Omit entirely if no date.\n\
         - `client_visible` — optional. `true` means the task is\n\
           visible to the client on their portal; `false` means\n\
           consultant-only. Defaults to the engagement setting.\n\
         - `description` — optional, short. Long context goes in\n\
           the body below the frontmatter, not here.\n\
         - `assignee` — one of `consultant` · `claude` · `client`.\n\
           Always set this explicitly. The system-level default\n\
           (when the field is omitted) is `claude` — which is often\n\
           WRONG for tasks the consultant will actually work on.\n\
           If in doubt, set `assignee: consultant`.\n\n\
         ### Bulk task imports\n\n\
         When the consultant asks you to \"import these tasks into\n\
         the Kanban\", write one `02-tasks/<id>.md` file per task.\n\
         Do not batch them into a single file. Use sequential ids\n\
         if they came from a numbered list (e.g. `t-p1-001` through\n\
         `t-p1-018`). Write them serially — not in parallel — to\n\
         avoid swamping the filesystem watcher.\n\n\
         ### Updating a task\n\n\
         If the task already exists in `02-tasks/` (same `id`), read\n\
         it first, edit the frontmatter or body, and write it back\n\
         with the same `id`. The app merges changes into the Kanban.\n\
         NEVER delete the file to \"recreate\" a task — that erases\n\
         the task from the Kanban. Editing in place is the right move.\n\n\
         ## Naming\n\n\
         - `YYYY-MM-DD` uses local calendar date, zero-padded.\n\
         - `<slug>` is lowercase-hyphenated, 2–6 words.\n\
         - Sequential NNN is zero-padded to 3 digits (001, 002, …).\n\n\
         ## When in doubt\n\n\
         - If a note doesn't fit any folder, put it in `00-inbox/`\n\
           and note-to-self that it needs sorting.\n\
         - Prefer appending to today's daily note for quick captures\n\
           over creating new files.\n\
         - Never write files to the vault root. Always use a folder.\n\n\
         ## Session opening\n\n\
         The app hands you a proactive briefing at session start with\n\
         today's calendar, priority mail, active tasks, and recent\n\
         notes. Open with a short, decision-oriented take — not a\n\
         blank-slate question.\n"
    )
}

/// Ensure today's daily note exists — create from the daily-note
/// template if missing. Called at session spawn so the briefing's
/// "recent notes" section finds it.
///
/// Safe to call repeatedly: if the file exists, leaves it alone.
pub fn ensure_today_daily_note(vault_path: &std::path::Path) -> Result<Option<std::path::PathBuf>, String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let target = vault_path.join("daily-notes").join(format!("{today}.md"));
    if target.exists() {
        return Ok(None);
    }
    // Parent folder should exist from scaffold, but be defensive.
    if let Some(parent) = target.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("create daily-notes/: {e}"))?;
        }
    }
    let template_path = vault_path.join("_templates/daily-note.md");
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|e| format!("read daily-note template: {e}"))?
    } else {
        // Fallback template — keeps daily-note creation from failing
        // if _templates/ was manually deleted.
        "# {{date}}\n\n## What's on today\n\n## Decisions\n\n## Log\n\n## Tomorrow\n".to_string()
    };
    let rendered = template.replace("{{date}}", &today);
    fs::write(&target, rendered)
        .map_err(|e| format!("write daily-note: {e}"))?;
    Ok(Some(target))
}

/// Tauri command: scaffold + daily-note ensure in one call, driven
/// from the session-spawn path. Returns the vault path as a string
/// so the caller can confirm it resolved.
#[tauri::command]
pub async fn ensure_engagement_scaffold(
    client_slug: String,
) -> Result<String, String> {
    let vault = vault_path_for_slug(&client_slug)?;
    if !vault.exists() {
        fs::create_dir_all(&vault)
            .map_err(|e| format!("create vault base: {e}"))?;
    }
    scaffold_vault_idempotent(&vault, &client_slug)?;
    let _ = ensure_today_daily_note(&vault)?;
    Ok(vault.to_string_lossy().to_string())
}

/// Snapshot of the four evolving-memory files for an engagement.
/// Empty strings for missing files — Phase 4B's read path treats
/// missing and empty the same (just don't render the section).
#[derive(Debug, Serialize, Default)]
pub struct EngagementMemory {
    pub principles: String,
    pub lessons: String,
    pub relationships: String,
    pub context: String,
}

/// Read the four memory files for an engagement's vault. Called at
/// session boot from the briefing composer so Claude opens with
/// carryover context from prior sessions.
///
/// Design (Phase 4B, 2026-04-21):
///   - Missing `_memory/` folder → empty snapshot. Not an error.
///   - Each file is soft-capped at 32KB on read to keep the briefing
///     bounded; if a file is larger, we take the TAIL (most recent
///     content assumed to be at the bottom of append-only files).
///   - Never fails — any read error for any file is logged and the
///     corresponding field returns empty. The session must boot even
///     if `_memory/` is corrupted.
#[tauri::command]
pub async fn read_engagement_memory(
    client_slug: String,
) -> Result<EngagementMemory, String> {
    let vault = vault_path_for_slug(&client_slug)?;
    let mem_dir = vault.join("_memory");
    if !mem_dir.exists() {
        return Ok(EngagementMemory::default());
    }

    fn read_capped(path: &std::path::Path) -> String {
        const CAP: usize = 32 * 1024;
        match fs::read_to_string(path) {
            Ok(s) if s.len() <= CAP => s,
            Ok(s) => {
                // Take the last CAP bytes on a char boundary to avoid
                // splitting a multi-byte UTF-8 sequence.
                let start = s.len() - CAP;
                let mut adj = start;
                while adj < s.len() && !s.is_char_boundary(adj) {
                    adj += 1;
                }
                format!("…(earlier entries truncated)…\n{}", &s[adj..])
            }
            Err(_) => String::new(),
        }
    }

    Ok(EngagementMemory {
        principles: read_capped(&mem_dir.join("principles.md")),
        lessons: read_capped(&mem_dir.join("lessons.md")),
        relationships: read_capped(&mem_dir.join("relationships.md")),
        context: read_capped(&mem_dir.join("context.md")),
    })
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

        // Use file_type() (not metadata()) so we can detect symlinks
        // WITHOUT following them. Codex 2026-04-21 pre-push: following
        // symlinked directories can recurse through a cycle or into a
        // huge external tree (e.g. an Obsidian vault that symlinks a
        // shared library into a subfolder), hanging or stack-
        // overflowing session boot. Skip all symlinks defensively —
        // real vault content lives under the real vault root.
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk_markdown(root, &path, out)?;
        } else if ft.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    out.push((path, mtime, meta.len()));
                }
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

    #[test]
    fn scaffold_creates_all_expected_folders_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        scaffold_vault_idempotent(&vault, "acme-corp").unwrap();

        for dir in [
            "00-inbox",
            "01-meetings",
            "02-tasks",
            "03-deliverables",
            "04-reference",
            "05-decisions",
            "06-briefs",
            "daily-notes",
            "_memory",
            "_memory/archive",
            "_templates",
            ".obsidian",
        ] {
            assert!(
                vault.join(dir).exists(),
                "expected dir {dir} to exist"
            );
        }
        for file in [
            "_templates/meeting-note.md",
            "_templates/task-note.md",
            "_templates/daily-note.md",
            "_templates/decision-note.md",
            "_templates/brief-note.md",
            "CLAUDE.md",
            "README.md",
            ".obsidian/app.json",
        ] {
            assert!(
                vault.join(file).exists(),
                "expected file {file} to exist"
            );
        }
        let claude_md = fs::read_to_string(vault.join("CLAUDE.md")).unwrap();
        assert!(claude_md.contains("acme-corp"));
        assert!(claude_md.contains("daily-notes/"));
        // Regression guard: CLAUDE.md must teach Claude the Kanban
        // task-creation workflow (frontmatter format + path). This
        // was missing in the Phase-C version and caused a real
        // user-facing bug where Claude used its internal TodoWrite
        // instead of writing files to 02-tasks/.
        assert!(
            claude_md.contains("02-tasks/<id>.md"),
            "CLAUDE.md should document the 02-tasks path"
        );
        assert!(
            claude_md.contains("id:") && claude_md.contains("title:") && claude_md.contains("status:"),
            "CLAUDE.md should show the required frontmatter fields"
        );
        assert!(
            claude_md.contains("in_progress"),
            "CLAUDE.md should list valid status values"
        );
        // Codex 2026-04-21: the CLAUDE.md must not claim the default
        // `assignee` is `consultant` — the actual code default is
        // `claude`. If this regresses to the wrong doc claim,
        // Claude-authored tasks will end up assigned to itself.
        assert!(
            !claude_md.contains("`consultant` (default)"),
            "CLAUDE.md must not claim default assignee is consultant"
        );
        // Scope the disclosure check to the `assignee` bullet itself
        // so a future edit that (e.g.) drops the disclosure but keeps
        // the word "default" elsewhere in the doc still fails this
        // test. Find the "- `assignee` —" bullet and inspect just the
        // ~300 chars of text that follow.
        let assignee_idx = claude_md
            .find("- `assignee` —")
            .expect("CLAUDE.md must contain an `assignee` bullet");
        let assignee_section_end = claude_md[assignee_idx..]
            .find("\n\n")
            .map(|off| assignee_idx + off)
            .unwrap_or(claude_md.len().min(assignee_idx + 500));
        let assignee_section = &claude_md[assignee_idx..assignee_section_end];
        assert!(
            assignee_section.contains("default") && assignee_section.contains("claude"),
            "assignee bullet must disclose that the omitted-field default is claude; \
             got:\n{assignee_section}"
        );
    }

    #[test]
    fn scaffold_preserves_user_edits_on_reinvoke() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        scaffold_vault_idempotent(&vault, "acme").unwrap();

        // User edits CLAUDE.md + adds a note template.
        fs::write(
            vault.join("CLAUDE.md"),
            "# My customised conventions\n",
        )
        .unwrap();
        fs::write(
            vault.join("_templates/meeting-note.md"),
            "# {{title}} custom\n",
        )
        .unwrap();

        // Re-run scaffolder — edits must survive.
        scaffold_vault_idempotent(&vault, "acme").unwrap();

        let claude_md = fs::read_to_string(vault.join("CLAUDE.md")).unwrap();
        assert_eq!(claude_md, "# My customised conventions\n");

        let tmpl = fs::read_to_string(vault.join("_templates/meeting-note.md")).unwrap();
        assert_eq!(tmpl, "# {{title}} custom\n");
    }

    #[test]
    fn ensure_today_daily_note_creates_then_skips() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        scaffold_vault_idempotent(&vault, "acme").unwrap();

        let first = ensure_today_daily_note(&vault).unwrap();
        assert!(first.is_some());
        let path = first.unwrap();
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(content.contains(&today), "daily note should contain today's date");

        // Second call — must not overwrite or re-create.
        let second = ensure_today_daily_note(&vault).unwrap();
        assert!(
            second.is_none(),
            "second call should return None (already exists)"
        );
    }

    #[test]
    fn ensure_today_daily_note_uses_fallback_when_template_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        // Minimum setup: just the daily-notes folder, no _templates.
        fs::create_dir_all(vault.join("daily-notes")).unwrap();
        let created = ensure_today_daily_note(&vault).unwrap().unwrap();
        let content = fs::read_to_string(&created).unwrap();
        assert!(content.contains("What's on today"));
    }

    #[cfg(unix)]
    #[test]
    fn walk_markdown_skips_symlinked_directories() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Real note in root.
        write_note(root, "a.md", "# real\n");

        // A sibling directory we'll point a symlink at. Put a file
        // inside it so we can tell whether the walk followed the link.
        let sibling = tmp.path().parent().unwrap().join("ikrs-walk-sibling");
        let _ = fs::remove_dir_all(&sibling);
        fs::create_dir_all(&sibling).unwrap();
        write_note(&sibling, "leaked.md", "# leaked\n");

        // symlink root/linkdir -> sibling
        symlink(&sibling, root.join("linkdir")).unwrap();

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

        // Real file found.
        assert!(names.iter().any(|n| n == "a.md"));
        // Symlink target content NOT reached — symlink skip prevents
        // traversal into `sibling/`.
        assert!(!names.iter().any(|n| n.contains("leaked.md")));

        // Cleanup sibling so parallel test runs don't collide.
        let _ = fs::remove_dir_all(&sibling);
    }
}

// ---------------------------------------------------------------------------
// Task 2: Orphan vault detection + import
// ---------------------------------------------------------------------------

/// A vault folder on disk that doesn't match any known engagement slug.
#[derive(Debug, Serialize, Clone)]
pub struct OrphanVault {
    pub slug: String,
    pub path: String,
    pub task_count: usize,
    pub last_modified: Option<String>,
}

/// Scan ~/.ikrs-workspace/vaults/ for folders whose slug doesn't
/// appear in `known_slugs`. Returns orphaned vaults with task counts.
///
/// `known_slugs` is passed from the frontend (which has the client
/// registry from Firestore) — keeps Rust free of Firestore deps.
#[tauri::command]
pub fn scan_orphan_vaults(known_slugs: Vec<String>) -> Result<Vec<OrphanVault>, String> {
    let base = vault_base();
    if !base.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&base)
        .map_err(|e| format!("read vaults dir: {e}"))?;

    let known: std::collections::HashSet<String> = known_slugs.into_iter().collect();
    let mut orphans = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Skip symlinks — prevent traversal via symlinked dirs
        if path.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
            continue;
        }
        let slug = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Skip dotfiles
        if slug.starts_with('.') {
            continue;
        }
        if known.contains(&slug) {
            continue;
        }

        // Count .md files in 02-tasks/
        let tasks_dir = path.join("02-tasks");
        let task_count = if tasks_dir.exists() {
            fs::read_dir(&tasks_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path().extension().and_then(|x| x.to_str()) == Some("md")
                                && !e.file_name().to_string_lossy().starts_with('.')
                        })
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };

        let last_modified = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| {
                let dur = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                Some(chrono::DateTime::from_timestamp(dur.as_secs() as i64, 0)?
                    .to_rfc3339())
            });

        orphans.push(OrphanVault {
            slug,
            path: path.to_string_lossy().to_string(),
            task_count,
            last_modified,
        });
    }

    Ok(orphans)
}

/// Import task .md files from an orphan vault into a destination vault.
/// Idempotent: skips files that already exist in the destination.
/// Returns the count of files imported.
#[tauri::command]
pub fn import_orphan_vault(
    source_slug: String,
    dest_slug: String,
) -> Result<ImportResult, String> {
    let source = vault_path_for_slug(&source_slug)?;
    let dest = vault_path_for_slug(&dest_slug)?;

    let src_tasks = source.join("02-tasks");
    let dst_tasks = dest.join("02-tasks");

    if !src_tasks.exists() {
        return Ok(ImportResult { imported: 0, skipped: 0 });
    }
    fs::create_dir_all(&dst_tasks)
        .map_err(|e| format!("create dest 02-tasks: {e}"))?;

    let mut imported = 0usize;
    let mut skipped = 0usize;

    let entries = fs::read_dir(&src_tasks)
        .map_err(|e| format!("read source 02-tasks: {e}"))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let name = match path.file_name() {
            Some(n) => n.to_owned(),
            None => continue,
        };
        if name.to_string_lossy().starts_with('.') {
            continue;
        }

        let target = dst_tasks.join(&name);
        if target.exists() {
            skipped += 1;
            continue;
        }

        fs::copy(&path, &target)
            .map_err(|e| format!("copy {}: {e}", name.to_string_lossy()))?;
        imported += 1;
    }

    Ok(ImportResult { imported, skipped })
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
}
