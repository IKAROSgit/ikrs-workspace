//! Google Calendar sync via direct REST API.
//!
//! Same architecture as gmail_sync (2026-04-20):
//!   - reads access token from keychain via refresh_if_needed
//!   - hits Calendar REST v3 directly (no MCP dependency)
//!   - discriminated result enum so callers branch on failure mode
//!   - sanitised errors (body logged only, never leaked to UI)
//!   - bounded concurrency (not needed here — single list call)
//!
//! Endpoint: GET calendars/primary/events
//!   - timeMin = now
//!   - timeMax = now + 30 days
//!   - singleEvents=true (expand recurring)
//!   - orderBy=startTime
//!   - maxResults = 50

use crate::commands::credentials::make_keychain_key;
use crate::oauth::token_refresh::refresh_if_needed;
use serde::Serialize;
use tauri::AppHandle;

const CALENDAR_BASE: &str = "https://www.googleapis.com/calendar/v3";

#[derive(Debug, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: String,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub attendees: Vec<String>,
    pub hangout_link: Option<String>,
    pub html_link: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum CalendarResult {
    Ok { events: Vec<CalendarEvent> },
    NotConnected,
    ScopeMissing,
    RateLimited,
    Network,
    Other { code: Option<u16> },
}

#[tauri::command]
pub async fn list_calendar_events(
    engagement_id: String,
    days_ahead: Option<u32>,
    max_results: Option<u32>,
    app: AppHandle,
) -> Result<CalendarResult, String> {
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let token = match refresh_if_needed(&keychain_key, &app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("list_calendar_events: no valid token — {e}");
            return Ok(CalendarResult::NotConnected);
        }
    };

    let days = days_ahead.unwrap_or(30).min(365);
    let limit = max_results.unwrap_or(50).min(250);
    let now = chrono::Utc::now();
    let later = now + chrono::Duration::days(days as i64);
    let time_min = now.to_rfc3339();
    let time_max = later.to_rfc3339();

    // Use calendarId=primary for the user's main calendar.
    let url = format!(
        "{CALENDAR_BASE}/calendars/primary/events\
         ?singleEvents=true\
         &orderBy=startTime\
         &timeMin={}\
         &timeMax={}\
         &maxResults={limit}",
        urlencoding::encode(&time_min),
        urlencoding::encode(&time_max),
    );

    let client = reqwest::Client::new();
    let resp = match client.get(&url).bearer_auth(&token).send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("calendar list transport error: {e}");
            return Ok(CalendarResult::Network);
        }
    };

    if !resp.status().is_success() {
        return Ok(classify_http_error(resp, "list").await);
    }

    #[derive(serde::Deserialize)]
    struct ListResp {
        items: Option<Vec<RawEvent>>,
    }
    #[derive(serde::Deserialize)]
    struct RawEvent {
        id: String,
        #[serde(default)]
        summary: String,
        #[serde(default)]
        location: Option<String>,
        #[serde(default)]
        status: String,
        #[serde(rename = "hangoutLink", default)]
        hangout_link: Option<String>,
        #[serde(rename = "htmlLink", default)]
        html_link: Option<String>,
        #[serde(default)]
        start: Option<TimeField>,
        #[serde(default)]
        end: Option<TimeField>,
        #[serde(default)]
        attendees: Option<Vec<Attendee>>,
    }
    #[derive(serde::Deserialize)]
    struct TimeField {
        #[serde(rename = "dateTime", default)]
        date_time: Option<String>,
        #[serde(default)]
        date: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Attendee {
        #[serde(default)]
        email: Option<String>,
        #[serde(rename = "displayName", default)]
        display_name: Option<String>,
    }

    let body: ListResp = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("calendar list JSON parse failed: {e}");
            return Ok(CalendarResult::Other { code: None });
        }
    };

    let events: Vec<CalendarEvent> = body
        .items
        .unwrap_or_default()
        .into_iter()
        .map(|e| {
            let start = e
                .start
                .as_ref()
                .and_then(|t| t.date_time.clone().or(t.date.clone()))
                .unwrap_or_default();
            let end = e
                .end
                .as_ref()
                .and_then(|t| t.date_time.clone().or(t.date.clone()))
                .unwrap_or_default();
            let attendees = e
                .attendees
                .unwrap_or_default()
                .into_iter()
                .filter_map(|a| a.display_name.or(a.email))
                .collect();
            CalendarEvent {
                id: e.id,
                summary: e.summary,
                start,
                end,
                location: e.location,
                attendees,
                hangout_link: e.hangout_link,
                html_link: e.html_link,
                status: e.status,
            }
        })
        .collect();

    Ok(CalendarResult::Ok { events })
}

/// Create a Calendar event on the user's primary calendar.
/// `start_iso` / `end_iso` are RFC 3339; all-day events not
/// supported in this first iteration (use timed events with UTC Z
/// suffix if caller wants full-day semantics for now).
#[tauri::command]
pub async fn create_calendar_event(
    engagement_id: String,
    summary: String,
    start_iso: String,
    end_iso: String,
    location: Option<String>,
    description: Option<String>,
    attendees: Vec<String>,
    app: AppHandle,
) -> Result<CreateEventResult, String> {
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let token = match refresh_if_needed(&keychain_key, &app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("create_calendar_event: no token — {e}");
            return Ok(CreateEventResult::NotConnected);
        }
    };

    if summary.trim().is_empty() {
        return Ok(CreateEventResult::Invalid {
            message: "Summary required".to_string(),
        });
    }
    if start_iso.trim().is_empty() || end_iso.trim().is_empty() {
        return Ok(CreateEventResult::Invalid {
            message: "Start and end required".to_string(),
        });
    }
    // Length caps — Google Calendar renders description as HTML in
    // its UI and in invitation emails. Even though consultants are
    // the authors, a Claude-drafted description pasted back in
    // could carry a phishing payload targeted at external invitees.
    // Capping length limits blast radius; full HTML sanitisation
    // is a deeper follow-up if needed.
    const MAX_SUMMARY: usize = 300;
    const MAX_LOCATION: usize = 1024;
    const MAX_DESCRIPTION: usize = 8192;
    if summary.chars().count() > MAX_SUMMARY {
        return Ok(CreateEventResult::Invalid {
            message: format!("Summary too long (max {MAX_SUMMARY} chars)"),
        });
    }
    if let Some(ref loc) = location {
        if loc.chars().count() > MAX_LOCATION {
            return Ok(CreateEventResult::Invalid {
                message: format!("Location too long (max {MAX_LOCATION} chars)"),
            });
        }
    }
    if let Some(ref desc) = description {
        if desc.chars().count() > MAX_DESCRIPTION {
            return Ok(CreateEventResult::Invalid {
                message: format!(
                    "Description too long (max {MAX_DESCRIPTION} chars)"
                ),
            });
        }
    }

    let mut body = serde_json::json!({
        "summary": summary,
        "start": { "dateTime": start_iso },
        "end": { "dateTime": end_iso },
    });
    if let Some(loc) = location.filter(|s| !s.trim().is_empty()) {
        body["location"] = serde_json::Value::String(loc);
    }
    if let Some(desc) = description.filter(|s| !s.trim().is_empty()) {
        body["description"] = serde_json::Value::String(desc);
    }
    if !attendees.is_empty() {
        body["attendees"] = serde_json::Value::Array(
            attendees
                .into_iter()
                .filter(|a| !a.trim().is_empty())
                .map(|email| serde_json::json!({ "email": email }))
                .collect(),
        );
    }

    let url = format!(
        "{CALENDAR_BASE}/calendars/primary/events?sendUpdates=all",
    );
    let client = reqwest::Client::new();
    let resp = match client
        .post(&url)
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("calendar create transport: {e}");
            return Ok(CreateEventResult::Network);
        }
    };

    let code = resp.status().as_u16();
    if !resp.status().is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        log::warn!(
            "calendar create HTTP {code} body: {}",
            truncate(&body_text, 400)
        );
        let lower = body_text.to_lowercase();
        return Ok(match code {
            401 => CreateEventResult::NotConnected,
            403 if lower.contains("insufficientpermissions")
                || lower.contains("scope_insufficient") =>
            {
                CreateEventResult::ScopeMissing
            }
            429 => CreateEventResult::RateLimited,
            _ => CreateEventResult::Other { code: Some(code) },
        });
    }

    #[derive(serde::Deserialize)]
    struct CreateResp {
        id: String,
        #[serde(rename = "htmlLink")]
        html_link: Option<String>,
    }
    let body: CreateResp = resp
        .json()
        .await
        .map_err(|e| format!("calendar create parse: {e}"))?;
    Ok(CreateEventResult::Ok {
        id: body.id,
        html_link: body.html_link.unwrap_or_default(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum CreateEventResult {
    Ok { id: String, html_link: String },
    NotConnected,
    ScopeMissing,
    RateLimited,
    Network,
    Invalid { message: String },
    Other { code: Option<u16> },
}

async fn classify_http_error(resp: reqwest::Response, op: &str) -> CalendarResult {
    let status = resp.status();
    let code = status.as_u16();
    let body = resp.text().await.unwrap_or_default();
    log::warn!("calendar {op} HTTP {code} body: {}", truncate(&body, 400));

    match code {
        401 => CalendarResult::NotConnected,
        403 => {
            let lower = body.to_lowercase();
            if lower.contains("insufficientpermissions")
                || lower.contains("scope_insufficient")
                || lower.contains("insufficient authentication scope")
            {
                CalendarResult::ScopeMissing
            } else if lower.contains("ratelimitexceeded") || lower.contains("quotaexceeded") {
                CalendarResult::RateLimited
            } else {
                CalendarResult::Other { code: Some(code) }
            }
        }
        429 => CalendarResult::RateLimited,
        _ => CalendarResult::Other { code: Some(code) },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_403_insufficient_permissions() {
        // Synthesise a classify_http_error decision without a real
        // reqwest::Response: since the body matching is the
        // interesting part, we can reproduce the logic here.
        fn classify(code: u16, body: &str) -> CalendarResult {
            match code {
                401 => CalendarResult::NotConnected,
                403 => {
                    let lower = body.to_lowercase();
                    if lower.contains("insufficientpermissions")
                        || lower.contains("scope_insufficient")
                        || lower.contains("insufficient authentication scope")
                    {
                        CalendarResult::ScopeMissing
                    } else if lower.contains("ratelimitexceeded")
                        || lower.contains("quotaexceeded")
                    {
                        CalendarResult::RateLimited
                    } else {
                        CalendarResult::Other { code: Some(code) }
                    }
                }
                429 => CalendarResult::RateLimited,
                _ => CalendarResult::Other { code: Some(code) },
            }
        }

        let scope_body = r#"{"error":{"code":403,"message":"Request had insufficient authentication scopes.","errors":[{"reason":"ACCESS_TOKEN_SCOPE_INSUFFICIENT"}]}}"#;
        assert!(matches!(
            classify(403, scope_body),
            CalendarResult::ScopeMissing
        ));

        let rate_body = r#"{"error":{"code":403,"errors":[{"reason":"rateLimitExceeded"}]}}"#;
        assert!(matches!(
            classify(403, rate_body),
            CalendarResult::RateLimited
        ));

        let quota_body = r#"{"error":{"code":403,"errors":[{"reason":"quotaExceeded"}]}}"#;
        assert!(matches!(
            classify(403, quota_body),
            CalendarResult::RateLimited
        ));

        let generic_body = r#"{"error":{"code":403,"message":"Forbidden"}}"#;
        assert!(matches!(
            classify(403, generic_body),
            CalendarResult::Other { code: Some(403) }
        ));

        assert!(matches!(
            classify(401, "any"),
            CalendarResult::NotConnected
        ));
        assert!(matches!(
            classify(429, "any"),
            CalendarResult::RateLimited
        ));
    }

    #[test]
    fn test_calendar_event_field_all_day_and_timed() {
        // All-day events come back with `date` only; timed events
        // come back with `dateTime`. Our flattening picks dateTime
        // first, falls back to date. This test asserts that shape.
        #[derive(serde::Deserialize)]
        struct TimeField {
            #[serde(rename = "dateTime", default)]
            date_time: Option<String>,
            #[serde(default)]
            date: Option<String>,
        }
        let timed: TimeField = serde_json::from_str(
            r#"{"dateTime":"2026-04-25T10:00:00+08:00","timeZone":"Asia/Dubai"}"#,
        )
        .unwrap();
        assert_eq!(
            timed.date_time.as_deref(),
            Some("2026-04-25T10:00:00+08:00")
        );
        assert_eq!(timed.date, None);

        let all_day: TimeField =
            serde_json::from_str(r#"{"date":"2026-04-25"}"#).unwrap();
        assert_eq!(all_day.date_time, None);
        assert_eq!(all_day.date.as_deref(), Some("2026-04-25"));

        // The mapping in list_calendar_events does
        // `date_time.or(date)` — assert that order.
        let pick = timed.date_time.clone().or(timed.date.clone());
        assert_eq!(pick.as_deref(), Some("2026-04-25T10:00:00+08:00"));
        let pick = all_day.date_time.clone().or(all_day.date.clone());
        assert_eq!(pick.as_deref(), Some("2026-04-25"));
    }

    #[test]
    fn test_calendar_result_serde() {
        for (v, tag) in [
            (
                CalendarResult::Ok {
                    events: vec![],
                },
                "ok",
            ),
            (CalendarResult::NotConnected, "not_connected"),
            (CalendarResult::ScopeMissing, "scope_missing"),
            (CalendarResult::RateLimited, "rate_limited"),
            (CalendarResult::Network, "network"),
            (
                CalendarResult::Other { code: Some(500) },
                "other",
            ),
        ] {
            let j = serde_json::to_string(&v).unwrap();
            assert!(
                j.contains(&format!("\"status\":\"{tag}\"")),
                "missing status={tag} in {j}"
            );
        }
    }
}
