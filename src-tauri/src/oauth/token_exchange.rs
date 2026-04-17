//! Token-endpoint POST for OAuth 2.0 authorization-code exchange.
//!
//! Factored out of `redirect_server.rs` and `identity_server.rs` so there
//! is a single place where the request shape is constructed. The "missing
//! client_secret" bug class bit the project twice on 2026-04-17 (identity
//! flow, then engagement flow) because each call site had its own copy
//! of the form-body construction and the fix to one did not propagate
//! to the other.
//!
//! This module ships with a self-contained mock HTTP server under
//! `#[cfg(test)]` so tests can assert the request shape (specifically:
//! `client_secret` IS present) without any network dependency. That is
//! the "real end-to-end dry-run against the token-endpoint contract"
//! that CODEX.md Gate 6 requires for auth work.

/// Parameters for the `authorization_code` grant type. All fields are
/// required by Google's Desktop-app OAuth clients — PKCE (`code_verifier`)
/// does NOT remove the need for `client_secret` on the wire.
pub struct AuthorizationCodeRequest<'a> {
    /// The token endpoint URL. Production:
    /// `https://oauth2.googleapis.com/token`. Tests point this at the
    /// mock server.
    pub endpoint: &'a str,
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub redirect_uri: &'a str,
    pub code: &'a str,
    pub verifier: &'a str,
}

/// Perform the authorization-code token exchange. On HTTP success,
/// returns the parsed JSON body; on failure, returns a human-readable
/// error string suitable for surfacing to the user.
pub async fn exchange_authorization_code(
    req: AuthorizationCodeRequest<'_>,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(req.endpoint)
        .form(&[
            ("code", req.code),
            ("client_id", req.client_id),
            ("client_secret", req.client_secret),
            ("redirect_uri", req.redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", req.verifier),
        ])
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange HTTP error: {body}"));
    }

    resp.json()
        .await
        .map_err(|e| format!("Token endpoint returned non-JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Captures what the code-under-test POSTed.
    #[derive(Default, Debug, Clone)]
    struct CapturedRequest {
        method: Option<String>,
        path: Option<String>,
        body: Option<String>,
    }

    /// Spin up a one-shot mock HTTP server on a random loopback port.
    /// The server accepts a single request, captures it, and replies
    /// with `response_json`. Returns (url, capture_handle).
    async fn start_mock_token_endpoint(
        response_json: &'static str,
    ) -> (String, Arc<Mutex<CapturedRequest>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}/token");
        let captured: Arc<Mutex<CapturedRequest>> = Arc::new(Mutex::new(CapturedRequest::default()));
        let captured_for_task = Arc::clone(&captured);

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n > 0 {
                    let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let header_end = raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(n);
                    let (headers, body) = raw.split_at(header_end.min(raw.len()));
                    let first_line = headers.lines().next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();

                    let mut c = captured_for_task.lock().unwrap();
                    c.method = Some(method);
                    c.path = Some(path);
                    c.body = Some(body.to_string());
                }

                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_json.len(),
                    response_json
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        (url, captured)
    }

    /// Regression guard for the "missing client_secret" bug class that
    /// shipped twice on 2026-04-17 (identity flow, then engagement
    /// flow). If someone refactors token_exchange and drops the
    /// client_secret form field, this test fails loudly.
    #[tokio::test]
    async fn exchange_posts_client_secret_and_all_required_fields() {
        let (url, captured) = start_mock_token_endpoint(
            r#"{"access_token":"fake-access","refresh_token":"fake-refresh","id_token":"fake.id.token","expires_in":3600,"token_type":"Bearer"}"#,
        )
        .await;

        let result = exchange_authorization_code(AuthorizationCodeRequest {
            endpoint: &url,
            client_id: "TEST_CLIENT_ID",
            client_secret: "GOCSPX-test-secret-value",
            redirect_uri: "http://127.0.0.1:49152/oauth/callback",
            code: "TEST_AUTHORIZATION_CODE",
            verifier: "TEST_PKCE_VERIFIER",
        })
        .await;

        assert!(result.is_ok(), "expected success, got {result:?}");

        let c = captured.lock().unwrap();
        let body = c
            .body
            .as_ref()
            .expect("mock server never captured a body — did the HTTP client reach it?");
        let method = c
            .method
            .as_ref()
            .expect("mock server never captured method");
        let path = c.path.as_ref().expect("mock server never captured path");

        assert_eq!(method, "POST");
        assert_eq!(path, "/token");

        // The bug class: the form body MUST include client_secret. This
        // assertion would have failed loudly on both the 2026-04-17
        // bugs (identity + engagement) before they ever reached Moe.
        assert!(
            body.contains("client_secret=GOCSPX-test-secret-value"),
            "request body must include client_secret form field; body was: {body}"
        );

        // All other required fields for the authorization_code grant.
        assert!(body.contains("client_id=TEST_CLIENT_ID"));
        assert!(body.contains("grant_type=authorization_code"));
        assert!(body.contains("code=TEST_AUTHORIZATION_CODE"));
        assert!(body.contains("code_verifier=TEST_PKCE_VERIFIER"));
        assert!(body.contains("redirect_uri="));
    }

    #[tokio::test]
    async fn exchange_propagates_endpoint_http_error() {
        // Mock that returns a Google-shaped error body.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}/token");

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"error":"invalid_request","error_description":"client_secret is missing."}"#;
                let resp = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        let result = exchange_authorization_code(AuthorizationCodeRequest {
            endpoint: &url,
            client_id: "X",
            client_secret: "X",
            redirect_uri: "X",
            code: "X",
            verifier: "X",
        })
        .await;

        match result {
            Err(msg) => assert!(msg.contains("client_secret is missing"),
                "error message should surface Google's error body; got: {msg}"),
            Ok(_) => panic!("expected error from 400 response"),
        }
    }

    #[tokio::test]
    async fn exchange_returns_parsed_json_on_success() {
        let (url, _captured) = start_mock_token_endpoint(
            r#"{"access_token":"AT","expires_in":3600,"token_type":"Bearer"}"#,
        )
        .await;

        let result = exchange_authorization_code(AuthorizationCodeRequest {
            endpoint: &url,
            client_id: "X",
            client_secret: "X",
            redirect_uri: "X",
            code: "X",
            verifier: "X",
        })
        .await
        .unwrap();

        assert_eq!(result["access_token"].as_str(), Some("AT"));
        assert_eq!(result["expires_in"].as_i64(), Some(3600));
    }
}
