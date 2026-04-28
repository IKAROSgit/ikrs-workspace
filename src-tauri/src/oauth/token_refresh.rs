use tauri::{AppHandle, Manager};
use tauri_plugin_keyring::KeyringExt;

use crate::oauth::token_cache::{CachedToken, TokenCache};

const IKRS_SERVICE: &str = "ikrs-workspace";

/// Token payload stored as JSON in the keychain.
///
/// `client_secret` is included because Google's Desktop-app OAuth
/// endpoint requires it on every grant (authorization_code AND
/// refresh_token) — we store it alongside the tokens so the refresh
/// module can re-use it without the caller having to re-supply it.
/// Per Google's docs (the same ones cited in the other OAuth files),
/// the Desktop-client secret is explicitly NOT treated as confidential
/// and keychain storage matches the established pattern.
///
/// `#[serde(default)]` on `client_secret` preserves backwards
/// compatibility: pre-2026-04-18 payloads (stored before this field
/// was added) deserialize with an empty string. The downstream
/// refresh will fail with the user-readable "Google session expired.
/// Please re-authenticate." prompt, and the fresh OAuth flow writes
/// the new format. One-time re-auth friction for existing users.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TokenPayload {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}

impl TokenPayload {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at <= now + 300 // 5-minute buffer
    }
}

/// Read the token from keychain, refresh if expired, return a valid access_token.
/// Returns Err if no token exists, JSON is corrupt, or refresh fails.
///
/// Cache-first behaviour (2026-04-18): checks the in-memory
/// `TokenCache` before hitting the OS keychain. On macOS ad-hoc
/// builds the keychain access prompts for every read; caching the
/// short-lived access token in memory drops the prompt count from
/// N-per-spawn to 1-per-app-session for a given engagement. On
/// refresh-failure paths we do NOT explicitly evict the cache — the
/// expired entry is naturally ignored by `get_fresh` (which inline-
/// checks expiry), so subsequent calls retry from keychain and can
/// surface the re-auth prompt cleanly.
pub async fn refresh_if_needed(keychain_key: &str, app: &AppHandle) -> Result<String, String> {
    let cache: tauri::State<TokenCache> = app.state();

    // Fast path: cached token still within expiry.
    if let Some(cached) = cache.get_fresh(keychain_key).await {
        return Ok(cached.access_token);
    }

    let raw = app
        .keyring()
        .get_password(IKRS_SERVICE, keychain_key)
        .ok()
        .flatten()
        .ok_or("No Google token found. Please authenticate first.")?;

    let payload: TokenPayload = serde_json::from_str(&raw).map_err(|_| {
        "Google session expired. Please re-authenticate.".to_string()
    })?;

    if !payload.is_expired() {
        // Populate the cache so subsequent spawns in this app
        // session skip the keychain (and its macOS prompt) entirely.
        cache
            .insert(
                keychain_key.to_string(),
                CachedToken {
                    access_token: payload.access_token.clone(),
                    expires_at: payload.expires_at,
                },
            )
            .await;
        return Ok(payload.access_token);
    }

    // Token expired — refresh it via the shared token_exchange module.
    // If client_secret is missing (happens on payloads written before
    // 2026-04-18 — see TokenPayload docstring) fail early with the
    // user-readable re-auth prompt rather than letting Google return
    // `invalid_request: client_secret is missing.`
    if payload.client_secret.is_empty() {
        return Err(
            "Google session expired. Please re-authenticate.".to_string(),
        );
    }

    let json = crate::oauth::token_exchange::exchange_refresh_token(
        crate::oauth::token_exchange::RefreshTokenRequest {
            endpoint: "https://oauth2.googleapis.com/token",
            client_id: &payload.client_id,
            client_secret: &payload.client_secret,
            refresh_token: &payload.refresh_token,
        },
    )
    .await?;

    let new_access_token = json["access_token"]
        .as_str()
        .ok_or("Missing access_token in refresh response")?
        .to_string();
    let new_expires_in = json["expires_in"].as_i64().unwrap_or(3600);

    // Phase F fix: Google may rotate the refresh_token on access-token
    // refresh (Desktop-app OAuth clients enrolled in rotation since 2022).
    // If Google returns a new refresh_token, use it — the old one will be
    // invalidated after a grace period. If absent, keep the existing one.
    let new_refresh_token = json["refresh_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or(payload.refresh_token);

    let updated = TokenPayload {
        access_token: new_access_token.clone(),
        refresh_token: new_refresh_token,
        expires_at: chrono::Utc::now().timestamp() + new_expires_in,
        client_id: payload.client_id,
        client_secret: payload.client_secret,
    };

    let updated_json = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    app.keyring()
        .set_password(IKRS_SERVICE, keychain_key, &updated_json)
        .map_err(|e| format!("Keychain update failed: {e}"))?;

    // Mirror the post-refresh token into the cache so the next
    // refresh_if_needed call skips the keychain read + its prompt.
    cache
        .insert(
            keychain_key.to_string(),
            CachedToken {
                access_token: new_access_token.clone(),
                expires_at: updated.expires_at,
            },
        )
        .await;

    Ok(new_access_token)
}

/// Like `refresh_if_needed`, but returns the full TokenPayload so
/// callers that need `client_id`/`client_secret`/`refresh_token` can
/// wire them through (e.g. the gmail MCP spawn — see
/// `mcp_config::GoogleOAuthCreds` — which runs its own OAuth refresh
/// cycle and ignores a bare access_token). Single-implementation
/// re-use would be cleaner; kept as a parallel function for now
/// because the refresh-and-store flow returns a freshly-constructed
/// TokenPayload already, and we want the same cache-first / refresh
/// semantics without duplicating that logic into callers.
pub async fn get_payload_refresh_if_needed(
    keychain_key: &str,
    app: &AppHandle,
) -> Result<TokenPayload, String> {
    let raw = app
        .keyring()
        .get_password(IKRS_SERVICE, keychain_key)
        .ok()
        .flatten()
        .ok_or("No Google token found. Please authenticate first.")?;

    let mut payload: TokenPayload = serde_json::from_str(&raw).map_err(|_| {
        "Google session expired. Please re-authenticate.".to_string()
    })?;

    if payload.is_expired() {
        // Refresh grants a new access_token; delegate to the bare
        // `refresh_if_needed` which also updates the keychain and
        // cache. Then re-read.
        let new_access_token = refresh_if_needed(keychain_key, app).await?;
        payload.access_token = new_access_token;
        // expires_at in the struct is stale, but callers of
        // get_payload_refresh_if_needed don't use it — they want
        // the static client_id/secret/refresh_token values.
    } else {
        // Mirror `refresh_if_needed`'s cache-warm behaviour on the
        // fresh-token path so the next refresh_if_needed skips the
        // keychain prompt.
        let cache: tauri::State<TokenCache> = app.state();
        cache
            .insert(
                keychain_key.to_string(),
                CachedToken {
                    access_token: payload.access_token.clone(),
                    expires_at: payload.expires_at,
                },
            )
            .await;
    }

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_not_expired() {
        let payload = TokenPayload {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
        };
        assert!(!payload.is_expired());
    }

    #[test]
    fn test_payload_expired() {
        let payload = TokenPayload {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now().timestamp() - 100,
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
        };
        assert!(payload.is_expired());
    }

    #[test]
    fn test_payload_expired_within_buffer() {
        let payload = TokenPayload {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now().timestamp() + 200, // Within 5-min buffer
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
        };
        assert!(payload.is_expired());
    }

    #[test]
    fn test_corrupted_json_is_handled() {
        let result: Result<TokenPayload, _> = serde_json::from_str("not-json");
        assert!(result.is_err());
    }

    #[test]
    fn test_plain_token_string_is_handled() {
        // Pre-Phase-4a format: plain access_token string
        let result: Result<TokenPayload, _> = serde_json::from_str("\"ya29.old-format-token\"");
        assert!(result.is_err());
    }
}
