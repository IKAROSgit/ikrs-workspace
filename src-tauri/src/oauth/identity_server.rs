//! OAuth 2.0 PKCE flow for Firebase identity login.
//!
//! Parallel to `redirect_server.rs` (which handles per-engagement Google
//! access tokens for Gmail/Calendar/Drive MCP access), this module serves
//! a single purpose: obtain a Google-issued `id_token` (OIDC JWT) that
//! the frontend can hand to Firebase's `signInWithCredential()` to
//! establish an authenticated session.
//!
//! Key differences from `redirect_server.rs`:
//! - Requests OIDC scopes (`openid email profile`), not GMail/Calendar/Drive.
//! - Returns the `id_token` via a one-shot Tauri event
//!   (`firebase-auth:id-token-ready`) instead of writing to the OS keychain.
//!   Firebase manages the session after; we do not hold the id_token
//!   ourselves.
//! - Includes a random `state` parameter for CSRF protection.
//! - Includes a random `nonce` bound into the id_token for replay
//!   protection (Firebase Auth validates the nonce against the claims).
//!
//! Why separate from redirect_server.rs: mixing flows would require a
//! "purpose" enum switching post-exchange behaviour at a branch, which
//! is easy to misuse. Two parallel implementations with sharply
//! different output shapes are safer.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Generate a cryptographically-random URL-safe base64 string.
pub fn generate_random_b64(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn bind_with_fallback(preferred_port: u16) -> impl std::future::Future<Output = Result<(TcpListener, u16), String>> {
    async move {
        for port in preferred_port..=preferred_port + 10 {
            match TcpListener::bind(format!("127.0.0.1:{port}")).await {
                Ok(listener) => return Ok((listener, port)),
                Err(_) => continue,
            }
        }
        Err(format!(
            "Could not bind to any port in range {}-{} for Firebase identity callback",
            preferred_port,
            preferred_port + 10
        ))
    }
}

struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn parse_callback(request: &str) -> CallbackParams {
    let mut out = CallbackParams {
        code: None,
        state: None,
        error: None,
    };
    let Some(path) = request.split_whitespace().nth(1) else {
        return out;
    };
    let Some(query) = path.split('?').nth(1) else {
        return out;
    };
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next().unwrap_or("");
        let v = parts
            .next()
            .map(|s| urlencoding::decode(s).unwrap_or_default().to_string())
            .unwrap_or_default();
        match k {
            "code" => out.code = Some(v),
            "state" => out.state = Some(v),
            "error" => out.error = Some(v),
            _ => {}
        }
    }
    out
}

// Neutral "received" wording so we do not claim success to the user
// before the backend token exchange has actually succeeded. If the
// exchange fails, the app surfaces a separate error; this page is
// merely an acknowledgement that Google's redirect landed on us.
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>IKAROS Workspace — Return to the app</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#f5f5f5}
.card{background:white;padding:2rem;border-radius:12px;box-shadow:0 2px 8px rgba(0,0,0,0.1);text-align:center;max-width:400px}
h1{color:#0f172a;font-size:1.25rem;margin:0 0 0.5rem}p{color:#666;margin:0}</style></head>
<body><div class="card"><h1>Sign-in received</h1><p>Return to IKAROS Workspace — it will confirm when the sign-in completes.</p></div></body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>IKAROS Workspace — Sign-in failed</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#f5f5f5}
.card{background:white;padding:2rem;border-radius:12px;box-shadow:0 2px 8px rgba(0,0,0,0.1);text-align:center;max-width:400px}
h1{color:#ef4444;font-size:1.5rem;margin:0 0 0.5rem}p{color:#666;margin:0}</style></head>
<body><div class="card"><h1>Sign-in failed</h1><p>You can close this tab and try again from IKAROS Workspace.</p></div></body></html>"#;

/// Starts a one-shot HTTP server that captures Google's OAuth redirect,
/// validates the `state` parameter against the one we issued, exchanges
/// the authorization code for tokens (including `id_token`), and emits
/// `firebase-auth:id-token-ready` with the id_token payload.
///
/// On any failure it emits `firebase-auth:error` with a human-readable
/// reason string; the frontend should listen for both and clean up on
/// either.
///
/// Returns (JoinHandle, actual_port).
pub async fn start_identity_redirect_server(
    preferred_port: u16,
    client_id: String,
    verifier: String,
    expected_state: String,
    expected_nonce: String,
    app: AppHandle,
) -> Result<(tokio::task::JoinHandle<Result<(), String>>, u16), String> {
    // Defensive — a zero-length expected_state would silently accept
    // any callback missing the `state` parameter when decode returns
    // an empty string. Callers should never do this; fail loudly.
    if expected_state.is_empty() {
        return Err("expected_state must not be empty".to_string());
    }

    let (listener, actual_port) = bind_with_fallback(preferred_port).await?;

    let handle = tokio::spawn(async move {
        let app_for_error = app.clone();

        // Helper: emit error event and return.
        let fail = |reason: String| async move {
            let _ = app_for_error.emit(
                "firebase-auth:error",
                serde_json::json!({ "reason": reason.clone() }),
            );
            Err::<(), String>(reason)
        };

        // Cap the accept() wait so the task cannot leak indefinitely
        // if the frontend's 5-minute setTimeout is somehow suspended
        // (tab backgrounded, system sleep). 310 s = frontend 5 min +
        // 10 s grace.
        let accept_result =
            tokio::time::timeout(std::time::Duration::from_secs(310), listener.accept()).await;
        let (mut stream, _addr) = match accept_result {
            Ok(Ok(ok)) => ok,
            Ok(Err(e)) => return fail(format!("Accept failed: {e}")).await,
            Err(_) => {
                return fail(
                    "Sign-in listener timed out after 310 s waiting for Google callback"
                        .to_string(),
                )
                .await
            }
        };

        let mut buf = vec![0u8; 4096];
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => return fail(format!("Read failed: {e}")).await,
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        let params = parse_callback(&request);

        // Google returned an explicit error (e.g. access_denied).
        if let Some(err) = params.error.clone() {
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                ERROR_HTML.len(),
                ERROR_HTML
            );
            let _ = stream.write_all(response.as_bytes()).await;
            drop(stream);
            return fail(format!("Google OAuth error: {err}")).await;
        }

        // CSRF check — `state` must match exactly what we issued.
        match params.state {
            Some(got) if got == expected_state => {}
            Some(got) => {
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    ERROR_HTML.len(),
                    ERROR_HTML
                );
                let _ = stream.write_all(response.as_bytes()).await;
                drop(stream);
                return fail(format!(
                    "State parameter mismatch (expected {expected_state}, got {got}) — possible CSRF"
                ))
                .await;
            }
            None => {
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    ERROR_HTML.len(),
                    ERROR_HTML
                );
                let _ = stream.write_all(response.as_bytes()).await;
                drop(stream);
                return fail("Missing state parameter in callback".to_string()).await;
            }
        }

        let code = match params.code {
            Some(c) => c,
            None => {
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    ERROR_HTML.len(),
                    ERROR_HTML
                );
                let _ = stream.write_all(response.as_bytes()).await;
                drop(stream);
                return fail("Missing authorization code in callback".to_string()).await;
            }
        };

        // Happy path: 200 response, browser shows success card.
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            SUCCESS_HTML.len(),
            SUCCESS_HTML
        );
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);

        // Exchange code → tokens.
        let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
        let http_client = reqwest::Client::new();
        let resp = match http_client
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
        {
            Ok(r) => r,
            Err(e) => return fail(format!("Token exchange request failed: {e}")).await,
        };

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return fail(format!("Token exchange HTTP error: {body}")).await;
        }

        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => return fail(format!("Token endpoint returned non-JSON: {e}")).await,
        };

        let id_token = match json["id_token"].as_str() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => {
                return fail(
                    "Token response missing id_token — is the `openid` scope requested?"
                        .to_string(),
                )
                .await
            }
        };

        // Firebase's signInWithCredential will validate the signature,
        // aud, iss, exp, and nonce on the client. We pass through and
        // include the nonce we expect so the frontend can spot-check.
        let _ = app.emit(
            "firebase-auth:id-token-ready",
            serde_json::json!({
                "id_token": id_token,
                "expected_nonce": expected_nonce,
            }),
        );

        Ok(())
    });

    Ok((handle, actual_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_callback_code_only() {
        let req = "GET /oauth/callback?code=abc&state=xyz HTTP/1.1\r\nHost: localhost\r\n";
        let p = parse_callback(req);
        assert_eq!(p.code.as_deref(), Some("abc"));
        assert_eq!(p.state.as_deref(), Some("xyz"));
        assert!(p.error.is_none());
    }

    #[test]
    fn test_parse_callback_error() {
        let req = "GET /oauth/callback?error=access_denied&state=xyz HTTP/1.1\r\n";
        let p = parse_callback(req);
        assert_eq!(p.error.as_deref(), Some("access_denied"));
        assert!(p.code.is_none());
    }

    #[test]
    fn test_parse_callback_url_encoded() {
        let req = "GET /oauth/callback?code=4%2F0AQlEd8x&state=ab-cd HTTP/1.1\r\n";
        let p = parse_callback(req);
        assert_eq!(p.code.as_deref(), Some("4/0AQlEd8x"));
        assert_eq!(p.state.as_deref(), Some("ab-cd"));
    }

    #[test]
    fn test_parse_callback_no_query() {
        let req = "GET /oauth/callback HTTP/1.1\r\n";
        let p = parse_callback(req);
        assert!(p.code.is_none());
        assert!(p.state.is_none());
        assert!(p.error.is_none());
    }

    #[test]
    fn test_generate_random_b64_lengths() {
        let a = generate_random_b64(32);
        let b = generate_random_b64(32);
        assert_ne!(a, b);
        // base64 URL-safe no-pad length for 32 bytes is 43
        assert_eq!(a.len(), 43);
    }
}
