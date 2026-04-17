use tauri::AppHandle;
use tauri_plugin_keyring::KeyringExt;

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
pub async fn refresh_if_needed(keychain_key: &str, app: &AppHandle) -> Result<String, String> {
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

    let updated = TokenPayload {
        access_token: new_access_token.clone(),
        refresh_token: payload.refresh_token, // Google doesn't always return a new refresh_token
        expires_at: chrono::Utc::now().timestamp() + new_expires_in,
        client_id: payload.client_id,
        client_secret: payload.client_secret,
    };

    let updated_json = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    app.keyring()
        .set_password(IKRS_SERVICE, keychain_key, &updated_json)
        .map_err(|e| format!("Keychain update failed: {e}"))?;

    Ok(new_access_token)
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
