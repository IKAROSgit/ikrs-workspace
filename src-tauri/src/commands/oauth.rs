use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, State};

#[derive(Default)]
pub struct OAuthState {
    pub pending_verifier: Mutex<Option<String>>,
    pub pending_server: Mutex<Option<tokio::task::JoinHandle<Result<(), String>>>>,
    // Firebase identity flow has its own slot to avoid cross-aborting the
    // engagement-OAuth flow when the two run concurrently.
    pub identity_pending_server: Mutex<Option<tokio::task::JoinHandle<Result<(), String>>>>,
}

#[derive(Serialize)]
pub struct OAuthFlowResult {
    pub auth_url: String,
    pub actual_port: u16,
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

// ---------------------------------------------------------------------------
// Firebase identity login via system-browser PKCE (Phase 4e / AuthProvider fix).
//
// Why separate from start_oauth_flow: that flow targets Gmail/Calendar/Drive
// API access and stores the resulting access_token in the OS keychain. This
// flow targets Firebase identity — scopes are OIDC (`openid email profile`),
// the output we care about is the `id_token` (which we emit via a one-shot
// Tauri event and never persist), and it includes `state` + `nonce` that
// the older engagement flow does not have.
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_firebase_identity_flow(
    client_id: String,
    redirect_port: u16,
    state: State<'_, OAuthState>,
    app: AppHandle,
) -> Result<OAuthFlowResult, String> {
    // Cancel any previous identity flow (leaves engagement flow alone).
    {
        let mut pending = state
            .identity_pending_server
            .lock()
            .map_err(|e| e.to_string())?;
        if let Some(handle) = pending.take() {
            handle.abort();
        }
    }

    let challenge = crate::oauth::pkce::generate_pkce();
    let csrf_state = crate::oauth::identity_server::generate_random_b64(32);
    let nonce = crate::oauth::identity_server::generate_random_b64(32);

    let (handle, actual_port) = crate::oauth::identity_server::start_identity_redirect_server(
        redirect_port,
        client_id.clone(),
        challenge.verifier,
        csrf_state.clone(),
        nonce.clone(),
        app,
    )
    .await?;

    {
        let mut pending = state
            .identity_pending_server
            .lock()
            .map_err(|e| e.to_string())?;
        *pending = Some(handle);
    }

    // OIDC scopes only — we intentionally do NOT ask for Gmail/Calendar/Drive
    // here. The per-engagement flow handles those scopes separately.
    let scope = "openid email profile";
    let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope={}&\
        state={}&\
        nonce={}&\
        code_challenge={}&\
        code_challenge_method=S256&\
        prompt=select_account",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(scope),
        urlencoding::encode(&csrf_state),
        urlencoding::encode(&nonce),
        challenge.challenge,
    );

    Ok(OAuthFlowResult {
        auth_url,
        actual_port,
    })
}

#[tauri::command]
pub async fn cancel_firebase_identity_flow(
    state: State<'_, OAuthState>,
) -> Result<(), String> {
    let mut pending = state
        .identity_pending_server
        .lock()
        .map_err(|e| e.to_string())?;
    if let Some(handle) = pending.take() {
        handle.abort();
    }
    Ok(())
}
