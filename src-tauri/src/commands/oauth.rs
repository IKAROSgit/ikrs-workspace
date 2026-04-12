use crate::oauth::pkce;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, State};

#[derive(Default)]
pub struct OAuthState {
    pub pending_verifier: Mutex<Option<String>>,
    pub pending_server: Mutex<Option<tokio::task::JoinHandle<Result<(), String>>>>,
}

#[derive(Serialize)]
pub struct OAuthStartResult {
    pub auth_url: String,
}

#[derive(Serialize)]
pub struct OAuthFlowResult {
    pub auth_url: String,
    pub actual_port: u16,
}

#[tauri::command]
pub async fn start_oauth(
    state: State<'_, OAuthState>,
    client_id: String,
    redirect_port: u16,
    scopes: Vec<String>,
) -> Result<OAuthStartResult, String> {
    let challenge = pkce::generate_pkce();

    let mut pending = state.pending_verifier.lock().map_err(|e| e.to_string())?;
    *pending = Some(challenge.verifier);

    let redirect_uri = format!("http://localhost:{redirect_port}/oauth/callback");
    let scope = scopes.join(" ");

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={client_id}&\
        redirect_uri={redirect_uri}&\
        response_type=code&\
        scope={scope}&\
        code_challenge={challenge}&\
        code_challenge_method=S256&\
        access_type=offline&\
        prompt=consent",
        client_id = urlencoding::encode(&client_id),
        redirect_uri = urlencoding::encode(&redirect_uri),
        scope = urlencoding::encode(&scope),
        challenge = challenge.challenge,
    );

    Ok(OAuthStartResult { auth_url })
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

#[tauri::command]
pub async fn exchange_oauth_code(
    state: State<'_, OAuthState>,
    code: String,
    client_id: String,
    redirect_port: u16,
) -> Result<TokenResponse, String> {
    let verifier = {
        let mut pending = state.pending_verifier.lock().map_err(|e| e.to_string())?;
        pending.take().ok_or("No pending OAuth flow")?
    };

    let redirect_uri = format!("http://localhost:{redirect_port}/oauth/callback");

    let client = reqwest::Client::new();
    let resp = client
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

    Ok(TokenResponse {
        access_token: json["access_token"]
            .as_str()
            .ok_or("Missing access_token")?
            .to_string(),
        refresh_token: json["refresh_token"].as_str().map(|s| s.to_string()),
        expires_in: json["expires_in"].as_u64().unwrap_or(3600),
    })
}

#[tauri::command]
pub async fn start_oauth_flow(
    engagement_id: String,
    client_id: String,
    redirect_port: u16,
    scopes: Vec<String>,
    state: State<'_, OAuthState>,
    app: AppHandle,
) -> Result<OAuthFlowResult, String> {
    // Cancel any pending flow
    {
        let mut pending = state.pending_server.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = pending.take() {
            handle.abort();
        }
    }

    let challenge = crate::oauth::pkce::generate_pkce();

    // Store verifier
    {
        let mut pending = state.pending_verifier.lock().map_err(|e| e.to_string())?;
        *pending = Some(challenge.verifier.clone());
    }

    // Build keychain key
    let keychain_key = format!("ikrs:{engagement_id}:google");

    // Start redirect server
    let (handle, actual_port) = crate::oauth::redirect_server::start_redirect_server(
        redirect_port,
        client_id.clone(),
        challenge.verifier,
        keychain_key,
        app,
    )
    .await?;

    // Store server handle for cancellation
    {
        let mut pending = state.pending_server.lock().map_err(|e| e.to_string())?;
        *pending = Some(handle);
    }

    // Build auth URL with actual port
    let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
    let scope = scopes.join(" ");
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope={}&\
        code_challenge={}&\
        code_challenge_method=S256&\
        access_type=offline&\
        prompt=consent",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&scope),
        challenge.challenge,
    );

    Ok(OAuthFlowResult {
        auth_url,
        actual_port,
    })
}

#[tauri::command]
pub async fn cancel_oauth_flow(
    state: State<'_, OAuthState>,
) -> Result<(), String> {
    let mut pending = state.pending_server.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = pending.take() {
        handle.abort();
    }
    let mut verifier = state.pending_verifier.lock().map_err(|e| e.to_string())?;
    *verifier = None;
    Ok(())
}
