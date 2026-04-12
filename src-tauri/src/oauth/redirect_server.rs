use tauri::{AppHandle, Emitter};
use tauri_plugin_keyring::KeyringExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const IKRS_SERVICE: &str = "ikrs-workspace";

/// Try to bind a TcpListener on localhost, scanning from preferred_port up to +10.
async fn bind_with_fallback(preferred_port: u16) -> Result<(TcpListener, u16), String> {
    for port in preferred_port..=preferred_port + 10 {
        match TcpListener::bind(format!("127.0.0.1:{port}")).await {
            Ok(listener) => return Ok((listener, port)),
            Err(_) => continue,
        }
    }
    Err(format!(
        "Could not bind to any port in range {}-{}",
        preferred_port,
        preferred_port + 10
    ))
}

/// Extract the `code` query parameter from an HTTP GET request line.
fn extract_code(request: &str) -> Option<String> {
    let path = request.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some("code") {
            return parts.next().map(|v| urlencoding::decode(v).unwrap_or_default().to_string());
        }
    }
    None
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>IKAROS Workspace</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#f5f5f5}
.card{background:white;padding:2rem;border-radius:12px;box-shadow:0 2px 8px rgba(0,0,0,0.1);text-align:center}
h1{color:#22c55e;font-size:1.5rem}p{color:#666}</style></head>
<body><div class="card"><h1>Sign-in complete</h1><p>You can close this tab and return to IKAROS Workspace.</p></div></body></html>"#;

/// Starts a one-shot HTTP server that captures the OAuth redirect code,
/// exchanges it for tokens, stores the access token in the keychain,
/// and emits `oauth:token-stored`.
///
/// Returns (JoinHandle, actual_port).
pub async fn start_redirect_server(
    preferred_port: u16,
    client_id: String,
    verifier: String,
    keychain_key: String,
    app: AppHandle,
) -> Result<(tokio::task::JoinHandle<Result<(), String>>, u16), String> {
    let (listener, actual_port) = bind_with_fallback(preferred_port).await?;

    let handle = tokio::spawn(async move {
        let (mut stream, _addr) = listener
            .accept()
            .await
            .map_err(|e| format!("Accept failed: {e}"))?;

        let mut buf = vec![0u8; 4096];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("Read failed: {e}"))?;
        let request = String::from_utf8_lossy(&buf[..n]);

        let code = extract_code(&request)
            .ok_or_else(|| "No authorization code in redirect".to_string())?;

        // Send success response before exchanging (so browser shows result immediately)
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            SUCCESS_HTML.len(),
            SUCCESS_HTML
        );
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);

        // Exchange code for tokens (PKCE, no client_secret -- Desktop OAuth)
        let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
        let http_client = reqwest::Client::new();
        let resp = http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code.as_str()),
                ("client_id", client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("Token exchange failed: {e}"))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Token exchange error: {body}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let access_token = json["access_token"]
            .as_str()
            .ok_or("Missing access_token")?
            .to_string();
        let refresh_token = json["refresh_token"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let expires_in = json["expires_in"].as_i64().unwrap_or(3600);

        // Store as JSON payload in keychain (includes refresh_token for auto-refresh)
        let payload = crate::oauth::token_refresh::TokenPayload {
            access_token,
            refresh_token,
            expires_at: chrono::Utc::now().timestamp() + expires_in,
            client_id: client_id.clone(),
        };
        let payload_json = serde_json::to_string(&payload)
            .map_err(|e| format!("Failed to serialize token payload: {e}"))?;

        app.keyring()
            .set_password(IKRS_SERVICE, &keychain_key, &payload_json)
            .map_err(|e| format!("Keychain store failed: {e}"))?;

        // Emit event so frontend knows token is ready
        let _ = app.emit("oauth:token-stored", serde_json::json!({
            "keychain_key": keychain_key,
        }));

        Ok(())
    });

    Ok((handle, actual_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_valid() {
        let req = "GET /oauth/callback?code=4/0AQlEd8x&scope=email HTTP/1.1\r\nHost: localhost\r\n";
        assert_eq!(extract_code(req), Some("4/0AQlEd8x".to_string()));
    }

    #[test]
    fn test_extract_code_missing() {
        let req = "GET /oauth/callback?error=access_denied HTTP/1.1\r\n";
        assert_eq!(extract_code(req), None);
    }

    #[test]
    fn test_extract_code_encoded() {
        let req = "GET /oauth/callback?code=4%2F0AQlEd8x HTTP/1.1\r\n";
        assert_eq!(extract_code(req), Some("4/0AQlEd8x".to_string()));
    }
}
