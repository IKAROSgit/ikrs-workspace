//! Google Drive sync via direct REST API.
//!
//! Same pattern as gmail_sync / calendar_sync (2026-04-20).
//!
//! Endpoints:
//!   GET files?q=...&orderBy=modifiedTime desc&pageSize=N
//!     - list: `'me' in owners` (user's own files)
//!     - search: `name contains 'query' and trashed=false`
//!
//! Fields requested minimally for list UX: id, name, mimeType,
//! modifiedTime, size, webViewLink. No full file content download
//! — that's a per-click-open flow for later.

use crate::commands::credentials::make_keychain_key;
use crate::oauth::token_refresh::refresh_if_needed;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const DRIVE_BASE: &str = "https://www.googleapis.com/drive/v3";

#[derive(Debug, Serialize, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    #[serde(rename = "modifiedTime", default)]
    pub modified_time: String,
    pub size: Option<String>,
    #[serde(rename = "webViewLink")]
    pub web_view_link: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum DriveResult {
    Ok { files: Vec<DriveFile> },
    NotConnected,
    ScopeMissing,
    RateLimited,
    Network,
    Other { code: Option<u16> },
}

#[tauri::command]
pub async fn list_drive_files(
    engagement_id: String,
    query: Option<String>,
    max_results: Option<u32>,
    app: AppHandle,
) -> Result<DriveResult, String> {
    run(engagement_id, query, max_results, app).await
}

async fn run(
    engagement_id: String,
    query: Option<String>,
    max_results: Option<u32>,
    app: AppHandle,
) -> Result<DriveResult, String> {
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let token = match refresh_if_needed(&keychain_key, &app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("list_drive_files: no valid token — {e}");
            return Ok(DriveResult::NotConnected);
        }
    };

    let limit = max_results.unwrap_or(50).min(100);

    // Build Drive `q` expression. Empty query = recent own files.
    //   - trashed=false always
    //   - if query supplied, add `name contains '<escaped>'`
    let q = build_drive_query(query.as_deref());

    let fields = "files(id,name,mimeType,modifiedTime,size,webViewLink),nextPageToken";

    let url = format!(
        "{DRIVE_BASE}/files\
         ?q={}\
         &orderBy=modifiedTime desc\
         &pageSize={limit}\
         &fields={}\
         &supportsAllDrives=true\
         &includeItemsFromAllDrives=true",
        urlencoding::encode(&q),
        urlencoding::encode(fields),
    );

    let client = reqwest::Client::new();
    let resp = match client.get(&url).bearer_auth(&token).send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("drive list transport error: {e}");
            return Ok(DriveResult::Network);
        }
    };

    if !resp.status().is_success() {
        return Ok(classify_http_error(resp).await);
    }

    #[derive(serde::Deserialize)]
    struct ListResp {
        files: Option<Vec<DriveFile>>,
    }
    let body: ListResp = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("drive list JSON parse failed: {e}");
            return Ok(DriveResult::Other { code: None });
        }
    };

    Ok(DriveResult::Ok {
        files: body.files.unwrap_or_default(),
    })
}

async fn classify_http_error(resp: reqwest::Response) -> DriveResult {
    let status = resp.status();
    let code = status.as_u16();
    let body = resp.text().await.unwrap_or_default();
    log::warn!("drive list HTTP {code} body: {}", truncate(&body, 400));

    match code {
        401 => DriveResult::NotConnected,
        403 => {
            let lower = body.to_lowercase();
            if lower.contains("insufficientpermissions")
                || lower.contains("scope_insufficient")
                || lower.contains("insufficient authentication scope")
            {
                DriveResult::ScopeMissing
            } else if lower.contains("ratelimitexceeded") || lower.contains("quotaexceeded") {
                DriveResult::RateLimited
            } else {
                DriveResult::Other { code: Some(code) }
            }
        }
        429 => DriveResult::RateLimited,
        _ => DriveResult::Other { code: Some(code) },
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n - 3).collect();
        format!("{t}...")
    }
}

/// Build the Drive `q` expression for list_drive_files. Extracted
/// into a pure function so injection-attempt tests don't need to
/// hit the network.
///
/// Escapes single-quote (Drive's string-quote) and backslash
/// (Drive's escape char). No other escaping needed — Drive's
/// query language does not have dollar-sign expansion, shell
/// metas, etc. per
/// <https://developers.google.com/drive/api/guides/search-files>.
fn build_drive_query(query: Option<&str>) -> String {
    match query {
        Some(s) if !s.trim().is_empty() => {
            let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
            format!("name contains '{escaped}' and trashed=false")
        }
        _ => "'me' in owners and trashed=false".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_drive_query_empty() {
        let q = build_drive_query(None);
        assert_eq!(q, "'me' in owners and trashed=false");
        let q = build_drive_query(Some(""));
        assert_eq!(q, "'me' in owners and trashed=false");
        let q = build_drive_query(Some("   "));
        assert_eq!(q, "'me' in owners and trashed=false");
    }

    #[test]
    fn test_build_drive_query_simple() {
        let q = build_drive_query(Some("project plan"));
        assert_eq!(q, "name contains 'project plan' and trashed=false");
    }

    #[test]
    fn test_build_drive_query_escapes_single_quote() {
        // The classic injection attempt: close the string and inject
        // ` or `. After escape it must remain one literal string.
        let q = build_drive_query(Some("x' or '1'='1"));
        // Every original ' must be escaped as \'.
        assert_eq!(q, "name contains 'x\\' or \\'1\\'=\\'1' and trashed=false");
        // Sanity: the count of UN-escaped single-quotes is exactly 2
        // (the outer wrappers), which is Drive's expected contract.
        let mut prev = ' ';
        let unescaped = q
            .chars()
            .filter(|&c| {
                let keep = c == '\'' && prev != '\\';
                prev = c;
                keep
            })
            .count();
        assert_eq!(unescaped, 2, "exactly one quoted string expected");
    }

    #[test]
    fn test_build_drive_query_escapes_backslash() {
        let q = build_drive_query(Some("path\\to\\file"));
        // `\` → `\\` so the final query reads `name contains
        // 'path\\to\\file'` which Drive parses as the literal
        // three-segment string.
        assert_eq!(q, "name contains 'path\\\\to\\\\file' and trashed=false");
    }

    #[test]
    fn test_drive_result_serde() {
        for (v, tag) in [
            (
                DriveResult::Ok { files: vec![] },
                "ok",
            ),
            (DriveResult::NotConnected, "not_connected"),
            (DriveResult::ScopeMissing, "scope_missing"),
            (DriveResult::RateLimited, "rate_limited"),
            (DriveResult::Network, "network"),
            (DriveResult::Other { code: Some(500) }, "other"),
        ] {
            let j = serde_json::to_string(&v).unwrap();
            assert!(j.contains(&format!("\"status\":\"{tag}\"")), "missing {tag} in {j}");
        }
    }
}
