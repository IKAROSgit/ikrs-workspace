use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct McpServerEntry {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

#[derive(Serialize)]
struct McpConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerEntry>,
}

/// Generates `.mcp-config.json` in the engagement directory.
/// Returns the absolute path to the generated config file.
///
/// - `google_access_token`: if Some, includes Gmail, Calendar, Drive servers
///   with the actual token value substituted into their env fields.
///   **The actual token is written to disk** rather than a
///   `${GOOGLE_ACCESS_TOKEN}` placeholder. Codex adversarial audit S14
///   flagged the placeholder approach as unverified against Claude CLI's
///   actual mcp-config parsing behaviour (Gate 6 violation — we could
///   not confirm whether Claude CLI does `${VAR}` interpolation from
///   its own env into MCP-child env). Writing the concrete value makes
///   the behaviour deterministic: what's in the file is what the MCP
///   subprocess receives. Security tradeoff: the token lives briefly
///   on disk. Mitigated by (a) file is in `.gitignore`, (b) lives under
///   the user's own engagement dir, (c) file perms 600 via
///   `apply_restrictive_perms`, (d) regenerated on every session spawn
///   (short lifetime, and the token itself is a 1-hour access token
///   that the refresh module swaps out).
/// - `vault_path`: if Some and directory exists, includes Obsidian server
/// - `google_oauth`: if Some, includes gmail (uses full OAuth creds) and
///   drive (uses access_token). gmail's @shinzolabs package does its own
///   OAuth refresh cycle using CLIENT_ID/CLIENT_SECRET/REFRESH_TOKEN —
///   confirmed by inspecting `dist/config.js` 2026-04-18 after the
///   stream-json probe revealed it ignores `GOOGLE_ACCESS_TOKEN`.
pub struct GoogleOAuthCreds<'a> {
    pub access_token: &'a str,
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub refresh_token: &'a str,
}

pub fn generate_mcp_config(
    engagement_path: &Path,
    google_oauth: Option<&GoogleOAuthCreds<'_>>,
    vault_path: Option<&Path>,
    npx_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let npx_command = npx_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "npx".to_string());

    // PATH injection for every MCP env block.
    //
    // Root cause this addresses (diagnosed 2026-04-18 via direct Mac
    // SSH — 8 hrs after token-cache ship): Tauri GUI apps on macOS
    // inherit launchd's sparse PATH (`/usr/bin:/bin:/usr/sbin:/sbin`),
    // NOT the login-shell PATH. The claude CLI we spawn inherits that
    // sparse PATH and passes it to MCP subprocesses. When Claude spawns
    // `/usr/local/bin/npx @bitbonsai/mcpvault@0.11.0`, npx reads the
    // package's bin script, whose shebang is `#!/usr/bin/env node`.
    // `env` searches PATH for `node` — and `/usr/local/bin` (where
    // node lives on Intel Macs / Apple Silicon via official installer)
    // is NOT in PATH. `env: node: No such file or directory`. MCP
    // subprocess exits immediately. Claude reports `mcp_servers:
    // [{"name":"obsidian","status":"failed"}]` and every user message
    // comes back as `authentication_failed` because the tool runtime
    // never initialised.
    //
    // Fix: prepend the directory containing `npx` (almost always the
    // same directory as `node`) plus the common install locations to
    // PATH for each MCP env. Does not override caller-supplied env.
    let mut path_dirs = Vec::<String>::new();
    if let Some(npx) = npx_path {
        if let Some(parent) = npx.parent() {
            path_dirs.push(parent.to_string_lossy().to_string());
        }
    }
    // Homebrew Apple Silicon + Intel defaults, and system PATH.
    for p in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        if !path_dirs.iter().any(|d| d == p) {
            path_dirs.push(p.to_string());
        }
    }
    let injected_path = path_dirs.join(":");

    let mut servers = HashMap::new();

    if let Some(creds) = google_oauth {
        // Gmail MCP (2026-04-18 rewrite — see retrospective comment above
        // about @shinzolabs not reading GOOGLE_ACCESS_TOKEN): pass full
        // OAuth creds so the package can run its own refresh cycle.
        //
        // Custom PORT + AUTH_SERVER_PORT: @shinzolabs binds an HTTP
        // transport port unconditionally at startup (default 3000).
        // Port 3000 collides with common local dev servers (Next.js,
        // Grafana, Metabase, etc.). We pin to 53121/53122 in the private
        // IANA dynamic range. Tests below lock in that every gmail env
        // carries both port overrides — dropping either would re-expose
        // the EADDRINUSE failure mode that wedged "Connecting..." on
        // Moe's Mac 2026-04-18.
        //
        // TELEMETRY_ENABLED=false: this package phones home to
        // shinzolabs by default. Off for consultant NDA posture.
        //
        // Single-session limitation: max_sessions=1 in session_manager,
        // so two engagements spawning gmail simultaneously won't happen.
        // When that constraint lifts, replace the constant ports with
        // a per-engagement hash (engagement_id → port derivation).
        servers.insert(
            "gmail".to_string(),
            McpServerEntry {
                command: npx_command.clone(),
                args: vec!["@shinzolabs/gmail-mcp@1.7.4".to_string()],
                env: HashMap::from([
                    ("CLIENT_ID".to_string(), creds.client_id.to_string()),
                    (
                        "CLIENT_SECRET".to_string(),
                        creds.client_secret.to_string(),
                    ),
                    (
                        "REFRESH_TOKEN".to_string(),
                        creds.refresh_token.to_string(),
                    ),
                    ("PORT".to_string(), "53121".to_string()),
                    ("AUTH_SERVER_PORT".to_string(), "53122".to_string()),
                    ("TELEMETRY_ENABLED".to_string(), "false".to_string()),
                    ("PATH".to_string(), injected_path.clone()),
                ]),
            },
        );
        // NOTE 2026-04-18: @cocal/google-calendar-mcp is temporarily
        // REMOVED from the default MCP set. Root cause: this package
        // does NOT accept GOOGLE_ACCESS_TOKEN as env var (unlike our
        // gmail + drive + obsidian MCPs). Instead it wants
        // GOOGLE_OAUTH_CREDENTIALS pointing at a gcp-oauth.keys.json
        // file (Google OAuth client credentials format) and then
        // performs ITS OWN OAuth consent flow + token exchange +
        // stores tokens at ~/.config/google-calendar-mcp/tokens.json.
        //
        // Confirmed via direct `npx @cocal/google-calendar-mcp@2.6.1`
        // on Moe's Mac 2026-04-18:
        //   "Error loading OAuth keys: OAuth credentials not found.
        //    Please provide credentials using one of these methods:
        //      1. Environment variable:
        //         Set GOOGLE_OAUTH_CREDENTIALS to the path of your
        //         credentials file..."
        //
        // Making it work cleanly requires either (a) writing our own
        // gcp-oauth.keys.json from the OAuth client env vars plus
        // having the MCP do its own OAuth (which would interfere with
        // our per-engagement OAuth UX), or (b) pre-populating the
        // tokens.json file with our already-obtained access+refresh
        // tokens. Either is a modest engineering task but blocks Moe
        // from using the app daily RIGHT NOW (without this, Claude's
        // system.init never fires → UI wedges on "Connecting...").
        //
        // Follow-up work tracked: integrate calendar by writing the
        // tokens.json pre-population path when we can validate the
        // exact JSON schema against @cocal's TokenManager
        // implementation. Until then, consultants lose calendar
        // access inside the Claude session but can still use
        // Google Calendar directly in their browser.
        //
        // servers.insert("calendar", ... ) — intentionally omitted.
        servers.insert(
            "drive".to_string(),
            McpServerEntry {
                command: npx_command.clone(),
                args: vec!["@piotr-agier/google-drive-mcp@2.0.2".to_string()],
                env: HashMap::from([
                    (
                        "GOOGLE_ACCESS_TOKEN".to_string(),
                        creds.access_token.to_string(),
                    ),
                    ("PATH".to_string(), injected_path.clone()),
                ]),
            },
        );
    }

    if let Some(vp) = vault_path {
        if vp.exists() {
            servers.insert(
                "obsidian".to_string(),
                McpServerEntry {
                    command: npx_command.clone(),
                    // NOTE 2026-04-18: `@bitbonsai/mcpvault@1.3.0`
                    // (the version we previously pinned here) has
                    // NEVER been published to npm. The npm registry's
                    // `@bitbonsai/mcpvault` only has 0.8.2, 0.9.0,
                    // 0.10.0, 0.11.0. Trying to `npx @bitbonsai/
                    // mcpvault@1.3.0` returns `ETARGET No matching
                    // version found for @bitbonsai/mcpvault@1.3.0`
                    // and npm exits 1. Claude CLI's MCP init then
                    // waits indefinitely for the handshake that
                    // never comes, and the surrounding Gmail/
                    // Calendar/Drive spawns fail to progress as well
                    // (cascade). Moe hit this as a 20-hour
                    // "Connecting..." hang. Fixed: pin to the
                    // highest-published version instead. Every
                    // pinned version in this file should be checked
                    // against `npm view <pkg> versions` on first
                    // introduction — see CODEX.md Gate-6-equivalent
                    // rule for package pins, queued as a follow-up.
                    args: vec![
                        "@bitbonsai/mcpvault@0.11.0".to_string(),
                        vp.to_string_lossy().to_string(),
                    ],
                    env: HashMap::from([(
                        "PATH".to_string(),
                        injected_path.clone(),
                    )]),
                },
            );
        }
    }

    let config = McpConfig {
        mcp_servers: servers,
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize MCP config: {e}"))?;

    let config_path = engagement_path.join(".mcp-config.json");
    // Atomic-600 write (Codex S14 follow-up 2026-04-18): create the
    // tmp file directly with mode 0o600 using OpenOptions so no
    // umask-default 0644 window exists between creation and chmod.
    // Then rename into place. On Windows this falls through to a
    // standard write — Windows ACLs on a user profile already
    // restrict to the owner by default.
    let tmp_path = engagement_path.join(".mcp-config.json.tmp");
    write_with_restrictive_perms(&tmp_path, json.as_bytes())?;
    std::fs::rename(&tmp_path, &config_path)
        .map_err(|e| format!("Failed to rename MCP config: {e}"))?;

    Ok(config_path)
}

/// Create a file with permissions 0o600 on Unix (owner rw, no group /
/// other access) from the moment it exists. Avoids the TOCTOU window
/// that `write` + chmod has — the file is never world-readable even
/// for microseconds.
#[cfg(unix)]
fn write_with_restrictive_perms(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    // Best-effort removal in case a stale tmp lingers from a prior
    // crashed spawn; otherwise `create_new` below would refuse.
    let _ = std::fs::remove_file(path);

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("Failed to create MCP tmp file at 0600: {e}"))?;
    file.write_all(contents)
        .map_err(|e| format!("Failed to write MCP config: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Failed to fsync MCP config: {e}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_with_restrictive_perms(path: &Path, contents: &[u8]) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|e| format!("Failed to write MCP config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const FAKE_TOKEN: &str = "ya29.fake-access-token-for-tests";
    const FAKE_CLIENT_ID: &str = "test-client-id.apps.googleusercontent.com";
    const FAKE_CLIENT_SECRET: &str = "GOCSPX-fake-client-secret";
    const FAKE_REFRESH_TOKEN: &str = "1//fake-refresh-token";

    fn fake_creds() -> GoogleOAuthCreds<'static> {
        GoogleOAuthCreds {
            access_token: FAKE_TOKEN,
            client_id: FAKE_CLIENT_ID,
            client_secret: FAKE_CLIENT_SECRET,
            refresh_token: FAKE_REFRESH_TOKEN,
        }
    }

    #[test]
    fn test_generate_config_with_google_token() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let creds = fake_creds();
        let result = generate_mcp_config(dir.path(), Some(&creds), Some(&vault), None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("gmail"));
        assert!(servers.contains_key("drive"));
        assert!(servers.contains_key("obsidian"));
        // Calendar intentionally omitted — see comment on calendar
        // insertion in generate_mcp_config (2026-04-18 follow-up).
        assert!(!servers.contains_key("calendar"));
        assert_eq!(servers.len(), 3);
    }

    #[test]
    fn test_generate_config_drive_token_written_literally() {
        // S14 core regression guard: the actual access token appears
        // in drive's env, NOT the `${GOOGLE_ACCESS_TOKEN}` placeholder.
        // Codex-flagged fix 2026-04-17.
        //
        // Gmail is excluded from this check because @shinzolabs/gmail-mcp
        // doesn't read GOOGLE_ACCESS_TOKEN at all (2026-04-18 source
        // inspection) — it uses CLIENT_ID/CLIENT_SECRET/REFRESH_TOKEN
        // and runs its own refresh cycle. See separate gmail tests.
        let dir = TempDir::new().unwrap();
        let creds = fake_creds();
        let result = generate_mcp_config(dir.path(), Some(&creds), None, None);
        assert!(result.is_ok());

        let content = fs::read_to_string(result.unwrap()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let drive_token = &parsed["mcpServers"]["drive"]["env"]["GOOGLE_ACCESS_TOKEN"];
        assert_eq!(drive_token.as_str().unwrap(), FAKE_TOKEN);
        // Defensive: no placeholder string anywhere in the written file.
        assert!(!content.contains("${GOOGLE_ACCESS_TOKEN}"));
    }

    #[test]
    fn test_gmail_uses_oauth_creds_not_access_token() {
        // Regression guard for the 2026-04-18 gmail-mcp fix. The
        // @shinzolabs/gmail-mcp package ignores GOOGLE_ACCESS_TOKEN
        // entirely — it reads CLIENT_ID, CLIENT_SECRET, REFRESH_TOKEN
        // from env and runs its own OAuth refresh. Dropping any of
        // those three would regress gmail MCP to status:"failed".
        let dir = TempDir::new().unwrap();
        let creds = fake_creds();
        let result = generate_mcp_config(dir.path(), Some(&creds), None, None);
        let content = fs::read_to_string(result.unwrap()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let gmail_env = &parsed["mcpServers"]["gmail"]["env"];

        assert_eq!(gmail_env["CLIENT_ID"].as_str().unwrap(), FAKE_CLIENT_ID);
        assert_eq!(
            gmail_env["CLIENT_SECRET"].as_str().unwrap(),
            FAKE_CLIENT_SECRET
        );
        assert_eq!(
            gmail_env["REFRESH_TOKEN"].as_str().unwrap(),
            FAKE_REFRESH_TOKEN
        );
        // Gmail env must NOT carry GOOGLE_ACCESS_TOKEN — the MCP
        // doesn't read it, and leaking it here would be a pointless
        // secondary copy of the credential.
        assert!(
            gmail_env.get("GOOGLE_ACCESS_TOKEN").is_none(),
            "gmail MCP shouldn't receive GOOGLE_ACCESS_TOKEN (@shinzolabs ignores it)"
        );
    }

    #[test]
    fn test_gmail_custom_ports_pinned() {
        // Regression guard for the 2026-04-18 EADDRINUSE fix. Without
        // PORT + AUTH_SERVER_PORT overrides, @shinzolabs/gmail-mcp
        // binds :3000 which collides with common local dev servers
        // (Next.js default, Grafana, Metabase, the user's own web
        // projects...). The MCP crashes at startup with
        // `listen EADDRINUSE: address already in use :::3000` before
        // it can even respond to the MCP initialize handshake, and
        // Claude reports it as status:"failed".
        //
        // We pin to 53121 / 53122 in the IANA dynamic range. Dropping
        // either env var would re-expose the EADDRINUSE failure mode.
        let dir = TempDir::new().unwrap();
        let creds = fake_creds();
        let result = generate_mcp_config(dir.path(), Some(&creds), None, None);
        let content = fs::read_to_string(result.unwrap()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let gmail_env = &parsed["mcpServers"]["gmail"]["env"];

        assert_eq!(gmail_env["PORT"].as_str().unwrap(), "53121");
        assert_eq!(gmail_env["AUTH_SERVER_PORT"].as_str().unwrap(), "53122");
        // Also: telemetry must be disabled for the consultant-NDA posture.
        assert_eq!(gmail_env["TELEMETRY_ENABLED"].as_str().unwrap(), "false");
    }

    #[cfg(unix)]
    #[test]
    fn test_generate_config_file_perms_600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let creds = fake_creds();
        let result = generate_mcp_config(dir.path(), Some(&creds), None, None);
        let config_path = result.unwrap();
        let mode = fs::metadata(&config_path).unwrap().permissions().mode();
        // On Unix the low 9 bits are rwxrwxrwx; 0o600 = rw-------.
        assert_eq!(mode & 0o777, 0o600, "config file must be 0600 on Unix");
    }

    #[test]
    fn test_generate_config_no_token() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let result = generate_mcp_config(dir.path(), None, Some(&vault), None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(!servers.contains_key("gmail"));
        assert!(servers.contains_key("obsidian"));
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn test_generate_config_no_vault() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("missing_vault");

        let creds = fake_creds();
        let result = generate_mcp_config(dir.path(), Some(&creds), Some(&nonexistent), None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("gmail"));
        assert!(servers.contains_key("drive"));
        assert!(!servers.contains_key("obsidian"));
        assert!(!servers.contains_key("calendar"));
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn test_generate_config_empty() {
        let dir = TempDir::new().unwrap();

        let result = generate_mcp_config(dir.path(), None, None, None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 0);
    }

    #[test]
    fn test_path_injected_into_every_mcp_env() {
        // Regression for the silent-MCP-failure bug diagnosed 2026-04-18
        // via direct Mac SSH:
        //   - Tauri GUI on macOS spawns children with launchd's sparse
        //     PATH (no /usr/local/bin, no /opt/homebrew/bin).
        //   - Claude CLI spawns MCP subprocesses with an env derived
        //     from that sparse PATH + the per-MCP env block.
        //   - npx's shebang (`#!/usr/bin/env node`) searches PATH for
        //     `node`, finds nothing, exits. MCP never initialises.
        //   - Claude reports `mcp_servers: [{status:"failed"}]` and
        //     every user message returns `authentication_failed`.
        //
        // Fix (this file, 2026-04-18): every MCP env block contains a
        // PATH that starts with the dirname of the resolved npx, then
        // common install locations. This test locks in that every
        // configured MCP carries a non-empty PATH — missing PATH on
        // any server would regress the silent-failure bug.
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let fake_npx = PathBuf::from("/opt/homebrew/bin/npx");
        let creds = fake_creds();

        let result = generate_mcp_config(
            dir.path(),
            Some(&creds),
            Some(&vault),
            Some(&fake_npx),
        );
        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();

        for (name, entry) in servers {
            let path = entry["env"]["PATH"].as_str().unwrap_or_else(|| {
                panic!("MCP `{name}` missing PATH in env — would regress silent-failure bug")
            });
            assert!(
                path.contains("/opt/homebrew/bin"),
                "MCP `{name}` PATH should begin with npx parent dir, got `{path}`"
            );
            assert!(
                path.contains("/usr/local/bin"),
                "MCP `{name}` PATH should include /usr/local/bin fallback, got `{path}`"
            );
        }
    }

    #[test]
    fn test_path_present_without_npx_path_arg() {
        // Even when the caller can't resolve npx (edge case on fresh
        // installs where binary_resolver returns None), the PATH we
        // inject still carries the homebrew + /usr/local fallbacks so
        // the MCP subprocess has a chance of finding node via the
        // common install locations.
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let result = generate_mcp_config(dir.path(), None, Some(&vault), None);
        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let path = parsed["mcpServers"]["obsidian"]["env"]["PATH"]
            .as_str()
            .expect("obsidian PATH must be present even when npx_path is None");
        assert!(path.contains("/usr/local/bin"));
        assert!(path.contains("/opt/homebrew/bin"));
    }
}
