//! Evolving-memory distiller for Phase 4B.
//!
//! Called at session-end (kill, engagement switch, app exit) with
//! the session transcript. Spawns a one-shot `claude -p` subprocess
//! that reviews the transcript and emits YAML describing new
//! principles, lessons, relationships, and context to merge into
//! `_memory/`. Output YAML is parsed, validated, and merged into
//! the four memory files using atomic writes.
//!
//! Design decisions (2026-04-21):
//!
//! - **Transcript comes from the frontend.** The frontend already
//!   holds the full message stream in `useClaudeStore.messages`;
//!   passing it over IPC avoids duplicating state in Rust.
//!
//! - **Distiller uses Claude CLI, not an API call.** Reuses the
//!   user's already-authenticated Claude Max subscription —
//!   zero extra secrets, zero extra billing surface. Runs without
//!   MCP (`--no-mcp` equivalent: we simply don't pass `--mcp-config`)
//!   so it can't fan out to Gmail/Drive/Obsidian while reviewing.
//!
//! - **Append-only merges for v1.** `principles.md`, `lessons.md`,
//!   `relationships.md` all append new content under a session-
//!   dated sub-header. `context.md` is overwritten with the
//!   distiller's fresh summary (replaces stale state).
//!
//! - **Size-bounded.** After each merge, any file over ~200 lines
//!   has its oldest ~50 lines rotated to
//!   `_memory/archive/YYYY-MM.md`. Oldest = first-occurring.
//!
//! - **Silent failure.** A distiller failure (CLI error, malformed
//!   YAML, merge I/O error) is logged but never propagated to the
//!   user — the primary session has already ended and there's
//!   nothing to retry from the UI. The files remain untouched.
//!
//! - **Kill switch.** If the env var `IKRS_DISABLE_DISTILLER=1` is
//!   set, the distiller no-ops. Lets us disable in emergencies
//!   without a binary redeploy.

use serde::Deserialize;
use std::path::{Path, PathBuf};

const MEMORY_LINE_CAP: usize = 200;
const ROTATION_CHUNK: usize = 50;

/// Parsed output of the distiller's YAML emit. All fields optional
/// so the CLI can omit sections it has nothing new for.
#[derive(Debug, Deserialize, Default)]
struct DistilledUpdate {
    #[serde(default)]
    principles: Vec<String>,
    #[serde(default)]
    lessons: Vec<String>,
    #[serde(default)]
    relationships: Vec<String>,
    #[serde(default)]
    context_update: Option<String>,
}

/// Merge the distilled update into the four memory files. Public so
/// tests + the Tauri command can exercise both the merge path alone
/// (given a mocked update) and the full distill+merge.
pub fn apply_update(
    vault_path: &Path,
    update: &DistilledUpdate,
    session_stamp: &str,
) -> Result<(), String> {
    let mem_dir = vault_path.join("_memory");
    std::fs::create_dir_all(mem_dir.join("archive"))
        .map_err(|e| format!("ensure _memory/archive: {e}"))?;

    if !update.principles.is_empty() {
        append_under_header(
            &mem_dir.join("principles.md"),
            session_stamp,
            &update.principles,
        )?;
    }
    if !update.lessons.is_empty() {
        append_under_header(
            &mem_dir.join("lessons.md"),
            session_stamp,
            &update.lessons,
        )?;
    }
    if !update.relationships.is_empty() {
        append_under_header(
            &mem_dir.join("relationships.md"),
            session_stamp,
            &update.relationships,
        )?;
    }
    if let Some(ref ctx) = update.context_update {
        overwrite_context(&mem_dir.join("context.md"), ctx, session_stamp)?;
    }

    // After merges, enforce size cap with rotation. Run for all four
    // even if this update didn't touch them — older drift can make
    // them overdue for rotation.
    for name in ["principles.md", "lessons.md", "relationships.md"] {
        enforce_size_cap(&mem_dir, name)?;
    }
    // context.md is bounded by overwrite; no rotation needed.

    Ok(())
}

fn append_under_header(
    path: &Path,
    session_stamp: &str,
    items: &[String],
) -> Result<(), String> {
    let mut existing = std::fs::read_to_string(path).unwrap_or_default();
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    let mut block = String::new();
    block.push_str(&format!("\n## Session {session_stamp}\n\n"));
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Preserve multi-line items by indenting continuation lines
        // under the bullet. Keeps Obsidian rendering tidy.
        let mut lines = trimmed.lines();
        if let Some(first) = lines.next() {
            block.push_str(&format!("- {first}\n"));
            for line in lines {
                block.push_str(&format!("  {line}\n"));
            }
        }
    }
    let combined = existing + &block;
    atomic_write(path, combined.as_bytes())?;
    Ok(())
}

fn overwrite_context(
    path: &Path,
    content: &str,
    session_stamp: &str,
) -> Result<(), String> {
    let body = format!(
        "_Last updated: {session_stamp}_\n\n{}\n",
        content.trim()
    );
    atomic_write(path, body.as_bytes())?;
    Ok(())
}

fn enforce_size_cap(mem_dir: &Path, file_name: &str) -> Result<(), String> {
    let path = mem_dir.join(file_name);
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {file_name}: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= MEMORY_LINE_CAP {
        return Ok(());
    }
    // Rotate the oldest ROTATION_CHUNK lines (plus any partial
    // header they belong to) to archive/YYYY-MM.md.
    let cut = ROTATION_CHUNK.min(lines.len() - 10); // keep ≥10 lines
    let (to_archive, to_keep) = lines.split_at(cut);

    let month = chrono::Local::now().format("%Y-%m").to_string();
    let archive_path = mem_dir
        .join("archive")
        .join(format!("{month}.md"));
    let prior_archive = std::fs::read_to_string(&archive_path).unwrap_or_default();
    let archived = if prior_archive.is_empty() {
        format!(
            "# {file_name} — archived {month}\n\n{}\n",
            to_archive.join("\n")
        )
    } else {
        format!(
            "{}\n\n## (continued — {file_name})\n\n{}\n",
            prior_archive.trim_end(),
            to_archive.join("\n")
        )
    };
    atomic_write(&archive_path, archived.as_bytes())?;
    atomic_write(&path, to_keep.join("\n").as_bytes())?;
    Ok(())
}

/// Atomic write using the same backup-restore pattern as the task
/// watcher (task_watch.rs). Never leaves the file half-written; on
/// Windows-style replace failures, restores the original.
fn atomic_write(target: &Path, contents: &[u8]) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or_else(|| "target has no parent".to_string())?;
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("ensure parent dir: {e}"))?;
    let name = target
        .file_name()
        .ok_or_else(|| "target has no filename".to_string())?
        .to_string_lossy()
        .to_string();
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, contents).map_err(|e| format!("write tmp: {e}"))?;
    match std::fs::rename(&tmp, target) {
        Ok(()) => Ok(()),
        Err(_) if target.exists() => {
            let backup = dir.join(format!(".{name}.bak"));
            std::fs::rename(target, &backup)
                .map_err(|e| format!("backup existing: {e}"))?;
            match std::fs::rename(&tmp, target) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&backup);
                    Ok(())
                }
                Err(e) => {
                    let _ = std::fs::rename(&backup, target);
                    Err(format!("replace failed: {e}"))
                }
            }
        }
        Err(e) => Err(format!("rename tmp→target: {e}")),
    }
}

/// Spawn a one-shot `claude -p` call with the transcript piped to
/// stdin, capture the YAML output, parse into `DistilledUpdate`.
///
/// Returns `None` if the distiller is disabled (env kill switch) or
/// the call produced no usable output. Never panics; errors are
/// mapped to `Err(String)` so the caller can log and continue.
pub async fn distill(
    claude_path: &Path,
    transcript: &str,
) -> Result<Option<DistilledUpdate>, String> {
    if std::env::var("IKRS_DISABLE_DISTILLER").ok().as_deref() == Some("1") {
        log::info!("distiller disabled via IKRS_DISABLE_DISTILLER=1");
        return Ok(None);
    }
    if transcript.trim().is_empty() {
        return Ok(None);
    }

    let prompt = distiller_prompt();

    let mut cmd = tokio::process::Command::new(claude_path);
    cmd.arg("-p")
        .arg(&prompt)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn claude -p: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(transcript.as_bytes()).await;
        let _ = stdin.shutdown().await;
        drop(stdin);
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "distiller timed out after 90s".to_string())?
    .map_err(|e| format!("distiller child failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "distiller exited with status {:?}: {}",
            output.status.code(),
            stderr.chars().take(300).collect::<String>()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let yaml = extract_yaml_block(&stdout);
    if yaml.trim().is_empty() {
        log::info!("distiller emitted no YAML body");
        return Ok(None);
    }
    let update: DistilledUpdate =
        serde_yaml::from_str(&yaml).map_err(|e| format!("yaml parse: {e}"))?;

    // Guard: empty update → return None so caller doesn't bother merging.
    let any = !update.principles.is_empty()
        || !update.lessons.is_empty()
        || !update.relationships.is_empty()
        || update.context_update.is_some();
    if !any {
        return Ok(None);
    }
    Ok(Some(update))
}

fn distiller_prompt() -> String {
    r#"You are reviewing the transcript of a just-ended consulting session between an IKAROS consultant and you (Claude). The transcript is piped to stdin.

Extract ONLY genuinely new context worth remembering for future sessions. Output strict YAML inside a single fenced ```yaml block. Nothing before or after the fence. No commentary.

Schema:
  principles: [string]        # how the consultant likes to work — new entries only
  lessons:    [string]        # gotchas discovered this session — with a brief "why it matters"
  relationships: [string]     # new people learned about, one block of text per person
  context_update: string|null # a FULL replacement for the engagement's current context — or null if the prior context is still accurate

Rules:
- Skip anything already commonly known or already in the prior memory (you should infer that from the transcript's own references to prior context).
- No speculation. Only include what was said or clearly implied in THIS session.
- Be terse. One sentence per bullet is the target.
- If there's nothing new in a category, emit an empty list (or null for context_update). Do not emit made-up filler.
- Do not repeat bullets across categories.
- Never include secrets, tokens, credentials, client NDA content, or personal data beyond the consultant's explicit mention.

Example emit:
```yaml
principles:
  - Consultant prefers briefings under 120 words.
lessons:
  - Calendar API rate limit is 250 quota-units/user/sec — batch event fetches.
relationships:
  - Sarah Chen — BLR Ops Lead. Decision owner for venue logistics. Prefers email.
context_update: |
  BLR Phase-2 pre-production. Blocker: budget approval from finance.
  Next review meeting 2026-04-28.
```

Now emit YAML for the transcript on stdin.
"#.to_string()
}

fn extract_yaml_block(stdout: &str) -> String {
    // Find the first ```yaml fence and the next ``` closing fence.
    if let Some(start) = stdout.find("```yaml") {
        let after = &stdout[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Fallback: maybe the model skipped the language hint.
    if let Some(start) = stdout.find("```") {
        let after = &stdout[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Last resort: treat entire stdout as potential YAML.
    stdout.trim().to_string()
}

/// Entry point used by the Tauri command.
pub async fn distill_and_persist(
    vault_path: PathBuf,
    transcript: String,
    claude_path: PathBuf,
) -> Result<bool, String> {
    let update = match distill(&claude_path, &transcript).await? {
        Some(u) => u,
        None => return Ok(false),
    };
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    apply_update(&vault_path, &update, &stamp)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_yaml_fenced_block() {
        let out = "some preamble\n```yaml\nprinciples:\n  - foo\n```\nepilogue";
        let got = extract_yaml_block(out);
        assert_eq!(got, "principles:\n  - foo");
    }

    #[test]
    fn extract_yaml_unfenced_block() {
        let out = "```\nprinciples:\n  - bar\n```";
        let got = extract_yaml_block(out);
        assert_eq!(got, "principles:\n  - bar");
    }

    #[test]
    fn extract_yaml_no_fence_fallback() {
        let out = "principles:\n  - baz\n";
        let got = extract_yaml_block(out);
        assert_eq!(got, "principles:\n  - baz");
    }

    #[test]
    fn apply_update_appends_principles() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault.join("_memory")).unwrap();
        let update = DistilledUpdate {
            principles: vec!["Prefers terse responses".to_string()],
            ..Default::default()
        };
        apply_update(&vault, &update, "2026-04-21 14:00").unwrap();
        let p = std::fs::read_to_string(vault.join("_memory/principles.md")).unwrap();
        assert!(p.contains("Session 2026-04-21 14:00"));
        assert!(p.contains("Prefers terse responses"));
    }

    #[test]
    fn apply_update_preserves_prior_content() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault.join("_memory")).unwrap();
        std::fs::write(
            vault.join("_memory/lessons.md"),
            "# Lessons\n\n## Session 2026-04-20 10:00\n- prior lesson\n",
        )
        .unwrap();
        let update = DistilledUpdate {
            lessons: vec!["new lesson".to_string()],
            ..Default::default()
        };
        apply_update(&vault, &update, "2026-04-21 14:00").unwrap();
        let p = std::fs::read_to_string(vault.join("_memory/lessons.md")).unwrap();
        assert!(p.contains("prior lesson"));
        assert!(p.contains("new lesson"));
    }

    #[test]
    fn context_update_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault.join("_memory")).unwrap();
        std::fs::write(
            vault.join("_memory/context.md"),
            "_Last updated: old_\n\nold context",
        )
        .unwrap();
        let update = DistilledUpdate {
            context_update: Some("NEW CONTEXT".to_string()),
            ..Default::default()
        };
        apply_update(&vault, &update, "2026-04-21").unwrap();
        let c = std::fs::read_to_string(vault.join("_memory/context.md")).unwrap();
        assert!(c.contains("NEW CONTEXT"));
        assert!(!c.contains("old context"));
        assert!(c.contains("Last updated: 2026-04-21"));
    }

    #[test]
    fn enforce_size_cap_rotates_to_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        let mem = vault.join("_memory");
        std::fs::create_dir_all(mem.join("archive")).unwrap();

        // Build a file of 250 lines — over the 200 cap.
        let body: String = (0..250)
            .map(|i| format!("- line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(mem.join("lessons.md"), &body).unwrap();

        enforce_size_cap(&mem, "lessons.md").unwrap();

        let remaining = std::fs::read_to_string(mem.join("lessons.md")).unwrap();
        let remaining_lines = remaining.lines().count();
        assert!(
            remaining_lines < 250,
            "expected rotation to reduce line count; got {remaining_lines}"
        );

        let archive_files: Vec<_> = std::fs::read_dir(mem.join("archive"))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(!archive_files.is_empty(), "archive should have ≥1 file");
    }

    #[test]
    fn empty_update_yields_no_files() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault.join("_memory")).unwrap();
        let update = DistilledUpdate::default();
        apply_update(&vault, &update, "2026-04-21").unwrap();
        // All four files absent — nothing was appended.
        for name in [
            "principles.md",
            "lessons.md",
            "relationships.md",
            "context.md",
        ] {
            assert!(
                !vault.join("_memory").join(name).exists(),
                "expected {name} to be absent"
            );
        }
    }
}
