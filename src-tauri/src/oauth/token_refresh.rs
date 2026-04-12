use tauri::AppHandle;
use tauri_plugin_keyring::KeyringExt;

const IKRS_SERVICE: &str = "ikrs-workspace";

/// Token payload stored as JSON in the keychain.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TokenPayload {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub client_id: String,
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

    // Token expired — refresh it
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", payload.client_id.as_str()),
            ("refresh_token", payload.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Google session expired. Please re-authenticate. ({body})"
        ));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
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
