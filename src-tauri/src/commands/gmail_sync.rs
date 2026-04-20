//! Gmail inbox sync via direct Gmail REST API.
//!
//! Background (2026-04-20): the `useGmail` frontend hook was stubbed
//! with a "MCP client bridge TODO" comment — Inbox view showed no
//! emails. Building an in-app MCP client just to talk to the same
//! gmail-mcp Claude spawns is overkill for a simple inbox view.
//!
//! This module bypasses MCP entirely and talks to the Gmail REST API
//! directly, using the per-engagement access token already in the
//! in-memory TokenCache / keychain. Authorisation is the same
//! OAuth scope set the user granted on "Connect Google".
//!
//! Endpoints used:
//!   GET users/me/messages?labelIds=INBOX&maxResults=N
//!   GET users/me/messages/{id}?format=metadata
//!     &metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date
//!
//! Metadata-only fetch to keep per-refresh bandwidth small.
//!
//! ## Error taxonomy (Codex 2026-04-20 review must-fix #2)
//!
//! Callers must distinguish "not connected yet" (show Connect-Google
//! UI) from "token expired and refresh failed" (show re-auth prompt)
//! from "rate limited" (show retry-later) from "network" (show
//! offline banner). We return a typed `GmailInboxResult` enum rather
//! than stringly-matched `Err(String)`.
//!
//! ## Security (Codex must-fix #3)
//!
//! Gmail's error response bodies can echo request URL / headers
//! which may include Authorization tokens (observed in some Google
//! 400-range responses). We log full bodies via `log::warn!` but
//! return only the HTTP status code to the frontend — nothing from
//! the response body ever leaves this module into UI text.
//!
//! ## Concurrency (Codex must-fix #4)
//!
//! Per-message metadata fetches are fan-out but capped at 8
//! concurrent calls via `futures::stream::buffer_unordered`. Prevents
//! rapid-refresh clicks from storming Google's 250 quota-units/sec
//! per-user limit.

use crate::commands::credentials::make_keychain_key;
use crate::oauth::token_refresh::refresh_if_needed;
use futures::stream::{self, StreamExt};
use serde::Serialize;
use tauri::AppHandle;

const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1";
const DEFAULT_MAX_RESULTS: u32 = 30;
const METADATA_CONCURRENCY: usize = 8;

#[derive(Debug, Serialize)]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub date: String,
    pub is_read: bool,
}

/// Discriminated result type so the frontend can branch on the
/// specific failure mode (see module docstring).
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum GmailInboxResult {
    /// Success path — zero or more messages.
    Ok { messages: Vec<GmailMessage> },
    /// No Google token in keychain for this engagement, OR the
    /// stored token's refresh grant failed (user needs to
    /// re-authenticate). Frontend should show Connect-Google UI.
    NotConnected,
    /// Token valid but missing gmail.readonly / gmail.modify scope.
    /// Frontend should show a scope-upgrade prompt — simple reconnect
    /// would loop unless scope changes are requested.
    ScopeMissing,
    /// Google is rate-limiting us. Back off and retry later.
    RateLimited,
    /// Network/transport layer failure (DNS, connection reset, TLS).
    Network,
    /// Anything else — generic error. `code` is the HTTP status if
    /// relevant, otherwise `None`.
    Other { code: Option<u16> },
}

/// List the most recent Inbox threads for the given engagement's
/// Google account.
#[tauri::command]
pub async fn list_gmail_inbox(
    engagement_id: String,
    max_results: Option<u32>,
    app: AppHandle,
) -> Result<GmailInboxResult, String> {
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let access_token = match refresh_if_needed(&keychain_key, &app).await {
        Ok(t) => t,
        Err(e) => {
            // Codex must-fix #5: don't pretend this is an empty inbox.
            log::info!("list_gmail_inbox: no valid token — {e}");
            return Ok(GmailInboxResult::NotConnected);
        }
    };

    let limit = max_results.unwrap_or(DEFAULT_MAX_RESULTS).min(100);
    let client = reqwest::Client::new();

    // Phase 1: list message IDs.
    let list_url = format!(
        "{GMAIL_BASE}/users/me/messages?labelIds=INBOX&maxResults={limit}",
    );
    let list_resp = match client
        .get(&list_url)
        .bearer_auth(&access_token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("gmail list transport error: {e}");
            return Ok(GmailInboxResult::Network);
        }
    };

    if !list_resp.status().is_success() {
        return Ok(classify_http_error(list_resp, "list").await);
    }

    #[derive(serde::Deserialize)]
    struct ListResp {
        messages: Option<Vec<ListItem>>,
    }
    #[derive(serde::Deserialize)]
    struct ListItem {
        id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    }
    let list_body: ListResp = match list_resp.json().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("gmail list JSON parse failed: {e}");
            return Ok(GmailInboxResult::Other { code: None });
        }
    };

    let items = match list_body.messages {
        Some(v) => v,
        None => {
            // Valid response, no INBOX messages. Surface as empty.
            return Ok(GmailInboxResult::Ok {
                messages: Vec::new(),
            });
        }
    };

    // Phase 2: bounded-concurrency metadata fetches.
    let fetched: Vec<_> = stream::iter(items)
        .map(|item| {
            let client = client.clone();
            let token = access_token.clone();
            async move {
                fetch_message_metadata(&client, &token, &item.id, &item.thread_id).await
            }
        })
        .buffer_unordered(METADATA_CONCURRENCY)
        .collect()
        .await;

    let mut out = Vec::with_capacity(fetched.len());
    for r in fetched {
        match r {
            Ok(msg) => out.push(msg),
            Err(e) => log::debug!("skip gmail message: {e}"),
        }
    }

    Ok(GmailInboxResult::Ok { messages: out })
}

/// Classify a non-2xx Gmail response into the structured result.
/// Reads and drops the response body to `log::warn!` only — never
/// returns body text to the UI, to avoid token / header leakage.
async fn classify_http_error(
    resp: reqwest::Response,
    op: &str,
) -> GmailInboxResult {
    let status = resp.status();
    let code = status.as_u16();
    let body = resp.text().await.unwrap_or_default();
    log::warn!("gmail {op} HTTP {code} body: {}", truncate(&body, 400));

    match code {
        401 => GmailInboxResult::NotConnected,
        403 => {
            // 403 could be scope missing OR rate-limit. Distinguish
            // via the body — Google uses "insufficientPermissions"
            // (or "ACCESS_TOKEN_SCOPE_INSUFFICIENT") for scope issues
            // and "rateLimitExceeded" / "userRateLimitExceeded" for
            // quota. We already have the body in-memory.
            let lower = body.to_lowercase();
            if lower.contains("insufficientpermissions")
                || lower.contains("scope_insufficient")
                || lower.contains("insufficient authentication scope")
            {
                GmailInboxResult::ScopeMissing
            } else if lower.contains("ratelimitexceeded") {
                GmailInboxResult::RateLimited
            } else {
                GmailInboxResult::Other { code: Some(code) }
            }
        }
        429 => GmailInboxResult::RateLimited,
        _ => GmailInboxResult::Other { code: Some(code) },
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

async fn fetch_message_metadata(
    client: &reqwest::Client,
    access_token: &str,
    id: &str,
    thread_id: &str,
) -> Result<GmailMessage, String> {
    // Note: metadataHeaders is a REPEATED query param (one per
    // header), not comma-separated. Google's API rejects the
    // comma form with 400.
    let url = format!(
        "{GMAIL_BASE}/users/me/messages/{id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date",
    );
    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("net: {e}"))?;

    if !resp.status().is_success() {
        // Don't propagate body here either — log + opaque error.
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        log::debug!("gmail get {id} HTTP {code}: {}", truncate(&body, 200));
        return Err(format!("HTTP {code}"));
    }

    #[derive(serde::Deserialize)]
    struct MsgResp {
        snippet: Option<String>,
        #[serde(rename = "labelIds", default)]
        label_ids: Vec<String>,
        payload: Option<Payload>,
    }
    #[derive(serde::Deserialize)]
    struct Payload {
        #[serde(default)]
        headers: Vec<Header>,
    }
    #[derive(serde::Deserialize)]
    struct Header {
        name: String,
        value: String,
    }

    let body: MsgResp = resp.json().await.map_err(|e| format!("parse: {e}"))?;

    let header_val = |key: &str| -> String {
        body.payload
            .as_ref()
            .and_then(|p| {
                p.headers
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case(key))
                    .map(|h| h.value.clone())
            })
            .unwrap_or_default()
    };

    Ok(GmailMessage {
        id: id.to_string(),
        thread_id: thread_id.to_string(),
        from: header_val("From"),
        subject: header_val("Subject"),
        snippet: body.snippet.unwrap_or_default(),
        date: header_val("Date"),
        is_read: !body.label_ids.iter().any(|l| l == "UNREAD"),
    })
}

/// Send an email via the Gmail REST API.
///
/// Uses the user's `me` account (the token's owner). Encodes the
/// RFC 2822 message as base64url per API contract. Scope required
/// is `gmail.modify` (we already grant that for Inbox reads) or
/// `gmail.send`. Our token has `gmail.modify` so this works without
/// a scope bump.
#[tauri::command]
pub async fn send_gmail_message(
    engagement_id: String,
    to: String,
    subject: String,
    body: String,
    cc: Option<String>,
    bcc: Option<String>,
    app: AppHandle,
) -> Result<SendResult, String> {
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let token = match refresh_if_needed(&keychain_key, &app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("send_gmail_message: no token — {e}");
            return Ok(SendResult::NotConnected);
        }
    };

    if to.trim().is_empty() {
        return Ok(SendResult::Invalid {
            message: "To address required".to_string(),
        });
    }

    let raw = build_rfc2822(&to, &subject, &body, cc.as_deref(), bcc.as_deref());
    let raw_b64url = base64_urlsafe_nopad(&raw);

    let url = format!("{GMAIL_BASE}/users/me/messages/send");
    let client = reqwest::Client::new();
    let resp = match client
        .post(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "raw": raw_b64url }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("gmail send transport error: {e}");
            return Ok(SendResult::Network);
        }
    };

    let code = resp.status().as_u16();
    if !resp.status().is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        log::warn!("gmail send HTTP {code} body: {}", truncate(&body_text, 400));
        return Ok(match code {
            401 => SendResult::NotConnected,
            403 if body_text.to_lowercase().contains("insufficientpermissions") => {
                SendResult::ScopeMissing
            }
            429 => SendResult::RateLimited,
            _ => SendResult::Other { code: Some(code) },
        });
    }

    #[derive(serde::Deserialize)]
    struct SendResp {
        id: String,
        #[serde(rename = "threadId")]
        thread_id: Option<String>,
    }
    let body: SendResp = resp
        .json()
        .await
        .map_err(|e| format!("gmail send parse: {e}"))?;
    Ok(SendResult::Ok {
        id: body.id,
        thread_id: body.thread_id.unwrap_or_default(),
    })
}

/// Mark a Gmail message as read by removing the `UNREAD` label.
#[tauri::command]
pub async fn mark_gmail_read(
    engagement_id: String,
    message_id: String,
    app: AppHandle,
) -> Result<SimpleResult, String> {
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let token = match refresh_if_needed(&keychain_key, &app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("mark_gmail_read: no token — {e}");
            return Ok(SimpleResult::NotConnected);
        }
    };
    let url = format!(
        "{GMAIL_BASE}/users/me/messages/{message_id}/modify",
        message_id = urlencoding::encode(&message_id),
    );
    let client = reqwest::Client::new();
    let resp = match client
        .post(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "removeLabelIds": ["UNREAD"] }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("gmail mark-read transport: {e}");
            return Ok(SimpleResult::Network);
        }
    };
    let code = resp.status().as_u16();
    if !resp.status().is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        log::warn!("gmail mark-read HTTP {code} body: {}", truncate(&body_text, 200));
        return Ok(match code {
            401 => SimpleResult::NotConnected,
            _ => SimpleResult::Other { code: Some(code) },
        });
    }
    Ok(SimpleResult::Ok)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SendResult {
    Ok { id: String, thread_id: String },
    NotConnected,
    ScopeMissing,
    RateLimited,
    Network,
    Invalid { message: String },
    Other { code: Option<u16> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SimpleResult {
    Ok,
    NotConnected,
    Network,
    Other { code: Option<u16> },
}

/// Build an RFC 2822 MIME message. Headers are ASCII-only here —
/// the body is UTF-8 and we mark the message as `text/plain;
/// charset="UTF-8"` with `Content-Transfer-Encoding: 8bit`. Subject
/// and To/Cc/Bcc values that contain non-ASCII are wrapped with
/// RFC 2047 encoded-word syntax (`=?UTF-8?B?...?=`) to keep the
/// header ASCII-clean.
fn build_rfc2822(
    to: &str,
    subject: &str,
    body: &str,
    cc: Option<&str>,
    bcc: Option<&str>,
) -> String {
    let mut headers = String::new();
    headers.push_str(&format!("To: {}\r\n", encode_header_value(to)));
    if let Some(c) = cc.filter(|s| !s.trim().is_empty()) {
        headers.push_str(&format!("Cc: {}\r\n", encode_header_value(c)));
    }
    if let Some(b) = bcc.filter(|s| !s.trim().is_empty()) {
        headers.push_str(&format!("Bcc: {}\r\n", encode_header_value(b)));
    }
    headers.push_str(&format!(
        "Subject: {}\r\n",
        encode_header_value(subject)
    ));
    headers.push_str("MIME-Version: 1.0\r\n");
    headers.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
    headers.push_str("Content-Transfer-Encoding: 8bit\r\n");
    headers.push_str("\r\n"); // end of headers
    headers.push_str(body);
    headers
}

/// Encode a header value using RFC 2047 if it contains non-ASCII.
/// Keeps pure-ASCII values unchanged (including display-name
/// formats like `Moe <moe@ikaros.ae>`).
fn encode_header_value(v: &str) -> String {
    if v.is_ascii() {
        v.to_string()
    } else {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(v.as_bytes());
        format!("=?UTF-8?B?{b64}?=")
    }
}

/// Base64url (no padding) — Gmail API's `raw` field format.
fn base64_urlsafe_nopad(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let s = "a".repeat(500);
        let t = truncate(&s, 50);
        assert_eq!(t.chars().count(), 50);
        assert!(t.ends_with("..."));
    }

    #[test]
    fn test_build_rfc2822_ascii() {
        let raw = build_rfc2822(
            "alice@example.com",
            "Hello",
            "Body text\nwith newline",
            None,
            None,
        );
        assert!(raw.contains("To: alice@example.com\r\n"));
        assert!(raw.contains("Subject: Hello\r\n"));
        assert!(raw.contains("MIME-Version: 1.0\r\n"));
        assert!(raw.contains("Content-Type: text/plain; charset=\"UTF-8\"\r\n"));
        assert!(raw.contains("\r\n\r\nBody text"));
        assert!(!raw.contains("Cc:"));
        assert!(!raw.contains("Bcc:"));
    }

    #[test]
    fn test_build_rfc2822_with_cc_bcc() {
        let raw = build_rfc2822(
            "a@x.com",
            "S",
            "B",
            Some("c@x.com"),
            Some("d@x.com"),
        );
        assert!(raw.contains("Cc: c@x.com\r\n"));
        assert!(raw.contains("Bcc: d@x.com\r\n"));
    }

    #[test]
    fn test_build_rfc2822_ignores_empty_cc() {
        let raw = build_rfc2822("a@x.com", "S", "B", Some(""), Some("   "));
        assert!(!raw.contains("Cc:"));
        assert!(!raw.contains("Bcc:"));
    }

    #[test]
    fn test_encode_header_value_rfc2047_for_nonascii() {
        // Arabic subject must be RFC 2047 encoded so the header
        // stays ASCII-clean on the wire.
        let enc = encode_header_value("مرحبا");
        assert!(enc.starts_with("=?UTF-8?B?"));
        assert!(enc.ends_with("?="));
        assert!(enc.is_ascii(), "encoded header must be pure ASCII: {enc}");
    }

    #[test]
    fn test_encode_header_value_passes_ascii_through() {
        let v = "Moe Aqeel <moe@ikaros.ae>";
        assert_eq!(encode_header_value(v), v);
    }

    #[test]
    fn test_base64_urlsafe_nopad() {
        // "Hello, world!" in base64url-nopad is SGVsbG8sIHdvcmxkIQ
        assert_eq!(
            base64_urlsafe_nopad("Hello, world!"),
            "SGVsbG8sIHdvcmxkIQ"
        );
        // No `=` padding, `+` → `-`, `/` → `_`. The input
        // `?>?>?>` produces bytes that b64-std would emit with a
        // `/` — here we expect `_` instead.
        let encoded = base64_urlsafe_nopad("?>?>?>");
        assert!(!encoded.contains('='));
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }

    #[test]
    fn test_error_variants_serialize_distinctly() {
        // Regression guard: the frontend switches on the `status`
        // tag, so each variant must serialize to its own discriminant.
        let cases: Vec<(GmailInboxResult, &str)> = vec![
            (
                GmailInboxResult::Ok {
                    messages: Vec::new(),
                },
                "ok",
            ),
            (GmailInboxResult::NotConnected, "not_connected"),
            (GmailInboxResult::ScopeMissing, "scope_missing"),
            (GmailInboxResult::RateLimited, "rate_limited"),
            (GmailInboxResult::Network, "network"),
            (GmailInboxResult::Other { code: Some(500) }, "other"),
        ];
        for (v, want) in cases {
            let j = serde_json::to_string(&v).unwrap();
            assert!(
                j.contains(&format!("\"status\":\"{want}\"")),
                "expected status=\"{want}\" in {j}"
            );
        }
    }
}
