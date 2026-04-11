use tauri_plugin_keyring::KeyringExt;

/// The keychain service name used for all IKAROS credentials.
const IKRS_SERVICE: &str = "ikrs-workspace";

/// Store a credential in the OS keychain.
/// The `key` becomes the keychain "user" under the "ikrs-workspace" service.
#[tauri::command]
pub async fn store_credential(
    app: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    app.keyring()
        .set_password(IKRS_SERVICE, &key, &value)
        .map_err(|e| format!("Failed to store credential: {e}"))
}

/// Retrieve a credential from the OS keychain.
/// Returns `None` if the key is not found.
#[tauri::command]
pub async fn get_credential(
    app: tauri::AppHandle,
    key: String,
) -> Result<Option<String>, String> {
    app.keyring()
        .get_password(IKRS_SERVICE, &key)
        .map_err(|e| format!("Failed to get credential: {e}"))
}

/// Delete a credential from the OS keychain.
/// Silently succeeds if the key does not exist.
#[tauri::command]
pub async fn delete_credential(
    app: tauri::AppHandle,
    key: String,
) -> Result<(), String> {
    match app.keyring().delete_password(IKRS_SERVICE, &key) {
        Ok(()) => Ok(()),
        Err(e) => {
            // keyring crate surfaces "not found" as NoEntry / NoStorageAccess depending on
            // the OS backend. Treat any error whose message contains "not found" or "no entry"
            // as a successful no-op so callers don't need to special-case missing keys.
            let msg = e.to_string().to_lowercase();
            if msg.contains("no entry") || msg.contains("not found") {
                Ok(())
            } else {
                Err(format!("Failed to delete credential: {e}"))
            }
        }
    }
}

/// Build the canonical keychain key for an engagement credential.
/// Format: `ikrs:{engagementId}:{provider}`
pub fn make_keychain_key(engagement_id: &str, provider: &str) -> String {
    format!("ikrs:{engagement_id}:{provider}")
}
