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

/// Dual-stack bind (IPv4 + IPv6) for the Firebase identity callback.
///
/// On macOS, `localhost` resolves to `::1` (IPv6) BEFORE `127.0.0.1`.
/// Google's OAuth redirects the user's browser to
/// `http://localhost:{port}/oauth/callback` — the browser picks
/// `::1` first. If this server only binds IPv4, the browser tries
/// to connect to `[::1]:{port}`, which is either (a) held by an
/// unrelated process like `rapportd` (Apple's AirDrop/Handoff
/// daemon) that listens on the IPv6 wildcard, or (b) nothing — in
/// which case the callback never reaches us. The Rust task waits
/// on its IPv4 listener until the 310 s timeout, the frontend
/// shows a "Complete the sign-in…" banner that never resolves, and
/// the consultant perceives "the button does nothing".
///
/// This was the exact root cause of the 2026-04-22 auth regression.
/// `redirect_server.rs` got the dual-stack fix in commit 78f807a
/// for the engagement-OAuth flow; this module was missed and still
/// bound IPv4-only. Now both modules share the same pattern: bind
/// both stacks when possible, accept from whichever the browser
/// hits first via `tokio::select!`.
async fn bind_with_fallback(
    preferred_port: u16,
) -> Result<(TcpListener, Option<TcpListener>, u16), String> {
    for port in preferred_port..=preferred_port + 10 {
        // Primary: IPv6 loopback (what macOS browsers prefer).
        let v6 = TcpListener::bind(format!("[::1]:{port}")).await;
        let v4 = TcpListener::bind(format!("127.0.0.1:{port}")).await;
        match (v6, v4) {
            (Ok(v6), Ok(v4)) => return Ok((v6, Some(v4), port)),
            (Ok(v6), Err(_)) => return Ok((v6, None, port)),
            (Err(_), Ok(v4)) => return Ok((v4, None, port)),
            (Err(_), Err(_)) => continue,
        }
    }
    Err(format!(
        "Could not bind to any port in range {}-{} for Firebase identity callback",
        preferred_port,
        preferred_port + 10
    ))
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
    client_secret: String,
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

    let (primary, secondary, actual_port) = bind_with_fallback(preferred_port).await?;

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
        //
        // Accept from whichever stack the browser hits first. When
        // `secondary` is None, only one stack is bound and we accept
        // on it alone. Mirrors redirect_server.rs pattern.
        let accept_future = async {
            match secondary {
                None => primary
                    .accept()
                    .await
                    .map_err(|e| format!("Accept failed: {e}")),
                Some(secondary) => {
                    tokio::select! {
                        r = primary.accept() => r.map_err(|e| format!("Accept v6 failed: {e}")),
                        r = secondary.accept() => r.map_err(|e| format!("Accept v4 failed: {e}")),
                    }
                }
            }
        };
        let accept_result =
            tokio::time::timeout(std::time::Duration::from_secs(310), accept_future).await;
        let (mut stream, _addr) = match accept_result {
            Ok(Ok(ok)) => ok,
            Ok(Err(e)) => return fail(e).await,
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

        // Exchange code → tokens via the shared token_exchange module.
        // See `docs/specs/m2-phase4e-firebase-identity-signin-design.md`
        // Amendment 2026-04-17 for the client_secret rationale. The
        // regression-guard test in `oauth::token_exchange` prevents a
        // future refactor from silently dropping the secret again.
        let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
        let json = match crate::oauth::token_exchange::exchange_authorization_code(
            crate::oauth::token_exchange::AuthorizationCodeRequest {
                endpoint: "https://oauth2.googleapis.com/token",
                client_id: &client_id,
                client_secret: &client_secret,
                redirect_uri: &redirect_uri,
                code: &code,
                verifier: &verifier,
            },
        )
        .await
        {
            Ok(j) => j,
            Err(e) => return fail(e).await,
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

    /// Regression guard for the 2026-04-22 auth bug: identity_server
    /// must bind IPv6 (::1) whenever the OS supports it, because
    /// macOS resolves `localhost` to `::1` first. A future refactor
    /// that silently drops the IPv6 bind would reproduce the bug.
    ///
    /// IPv4-only environments (locked-down CI, IPv6-disabled kernels)
    /// are detected via a capability probe: we first try to bind
    /// `::1` on a throwaway port. If that fails, the test is
    /// skipped with `eprintln!` rather than asserted — Codex
    /// 2026-04-22 pre-push P2: a hard assert here would produce
    /// spurious failures on IPv4-only hosts even though
    /// bind_with_fallback is correctly degrading.
    #[tokio::test]
    async fn bind_with_fallback_produces_ipv6_listener_when_available() {
        // Capability probe — does this host support IPv6 loopback
        // at all? Port 0 asks the OS for any free port.
        let ipv6_supported = TcpListener::bind("[::1]:0").await.is_ok();
        if !ipv6_supported {
            eprintln!(
                "bind_with_fallback_produces_ipv6_listener_when_available: \
                 skipping — host has no IPv6 loopback support"
            );
            return;
        }

        // Pick a high random port unlikely to collide with parallel
        // test runs or system services. Fallback range spans +10.
        let pref = 54321_u16;
        let (primary, secondary, _port) = bind_with_fallback(pref)
            .await
            .expect("bind should succeed on a free port");

        // At least one of the two listeners must be IPv6. On this
        // dual-stack host (confirmed by the probe above), the fix
        // must deliver an IPv6 bind — otherwise we've regressed
        // into the 2026-04-22 auth hang.
        let primary_is_v6 = primary
            .local_addr()
            .map(|a| a.is_ipv6())
            .unwrap_or(false);
        let secondary_is_v6 = secondary
            .as_ref()
            .and_then(|s| s.local_addr().ok())
            .map(|a| a.is_ipv6())
            .unwrap_or(false);
        assert!(
            primary_is_v6 || secondary_is_v6,
            "bind_with_fallback must produce at least one IPv6 listener on a dual-stack host"
        );
    }

    /// The fallback range is 10 ports wide; if all 11 are taken, we
    /// return Err rather than hang. This test pre-binds the entire
    /// range and asserts the Err.
    #[tokio::test]
    async fn bind_with_fallback_errors_when_range_exhausted() {
        let start = 54500_u16;
        // Hold 11 ports (preferred + 10 fallbacks) on both stacks
        // so bind_with_fallback can't get any of them.
        let mut holders = Vec::new();
        for p in start..=start + 10 {
            if let Ok(l) = TcpListener::bind(format!("[::1]:{p}")).await {
                holders.push(l);
            }
            if let Ok(l) = TcpListener::bind(format!("127.0.0.1:{p}")).await {
                holders.push(l);
            }
        }
        let got = bind_with_fallback(start).await;
        drop(holders);
        assert!(
            got.is_err(),
            "expected bind_with_fallback to Err when range is exhausted"
        );
    }
}
