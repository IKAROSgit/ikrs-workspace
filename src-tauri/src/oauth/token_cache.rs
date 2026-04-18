//! In-memory cache for Google OAuth access tokens, reducing keychain
//! prompt spam under macOS ad-hoc signing.
//!
//! Problem: on macOS, every keychain access from an app not in the
//! entry's ACL triggers a "Do you want to allow IKAROS Workspace to
//! access the keychain?" dialog. Ad-hoc signed dev builds produce a
//! different code signature on every rebuild, so the "Always Allow"
//! choice users make doesn't persist across rebuilds. Each call to
//! `refresh_if_needed` — triggered on every Claude session spawn —
//! hits the keychain again. Moe reported 5 prompts in a row during
//! normal use 2026-04-18.
//!
//! Solution: cache the access token + expiry timestamp in process
//! memory for the lifetime of the app. First spawn in a session
//! causes one keychain read + one prompt; subsequent spawns (same
//! engagement) hit the cache.
//!
//! Security posture:
//! - Cache is in-process only. Lost on app quit. No disk spill.
//! - Not persisted across app restarts — fresh keychain read (one
//!   prompt) after each launch.
//! - Evicted when the consultant signs out, engagement is deleted,
//!   token is refreshed, or manually (future cancel-flow).
//! - Tokens themselves already live on disk in two places (keychain
//!   + .mcp-config.json written to the engagement dir). Adding them
//!   to process memory is a minor marginal increase in exposure
//!   surface.
//! - Does NOT cache the refresh_token or client_secret — only the
//!   short-lived access_token. Refresh writes back to both keychain
//!   and cache.
//!
//! Not a replacement for real Apple Developer signing. Once the app
//! has a stable signing identity, macOS "Always Allow" persists
//! across rebuilds and this cache becomes a latency optimization
//! rather than a prompt-spam mitigation. Keeping the cache in place
//! either way — the latency win is real.

use std::collections::HashMap;
use tokio::sync::RwLock;

/// Snapshot of a cached access token + its expiry timestamp.
/// Cloneable because callers usually just need the token string and
/// we don't want them holding an async lock across an HTTP call.
#[derive(Clone, Debug)]
pub struct CachedToken {
    pub access_token: String,
    /// Unix timestamp (seconds) — absolute expiry from Google's
    /// token response. Same semantic as `TokenPayload.expires_at`.
    pub expires_at: i64,
}

impl CachedToken {
    /// Mirrors `TokenPayload::is_expired` — uses the same 5-minute
    /// grace buffer so the cache and the keychain-read path agree on
    /// when a token is "too close to expiry to use."
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at <= now + 300
    }
}

/// Tauri-managed state. Registered once in `lib.rs::.manage(...)`.
#[derive(Default)]
pub struct TokenCache {
    entries: RwLock<HashMap<String, CachedToken>>,
}

impl TokenCache {
    /// Fetch a cached token. Returns `Some(CachedToken)` if the entry
    /// exists AND is still within its expiry window; `None` otherwise.
    /// The expiry check is inline so callers never accidentally use a
    /// stale token just because the cache has it.
    pub async fn get_fresh(&self, key: &str) -> Option<CachedToken> {
        let entries = self.entries.read().await;
        let token = entries.get(key)?;
        if token.is_expired() {
            return None;
        }
        Some(token.clone())
    }

    /// Store (or overwrite) a token in the cache. Called after a
    /// successful keychain read OR after a successful refresh.
    pub async fn insert(&self, key: String, token: CachedToken) {
        self.entries.write().await.insert(key, token);
    }

    /// Evict a single entry. Called when an engagement is deleted or
    /// when a token refresh fails (so the next spawn re-reads from
    /// keychain and can surface the re-auth prompt).
    pub async fn evict(&self, key: &str) {
        self.entries.write().await.remove(key);
    }

    /// Clear the entire cache. Called on logOut so a subsequent
    /// sign-in with a different consultant doesn't see the prior
    /// consultant's cached tokens.
    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }

    /// Diagnostic: current entry count. Used by tests + optionally a
    /// "cache hit/miss" metric we might surface in the Settings
    /// disclosure page later.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(value: &str, expires_in_secs: i64) -> CachedToken {
        CachedToken {
            access_token: value.to_string(),
            expires_at: chrono::Utc::now().timestamp() + expires_in_secs,
        }
    }

    #[tokio::test]
    async fn get_fresh_returns_none_for_missing_key() {
        let cache = TokenCache::default();
        assert!(cache.get_fresh("missing").await.is_none());
    }

    #[tokio::test]
    async fn insert_and_get_roundtrip() {
        let cache = TokenCache::default();
        cache.insert("k1".to_string(), token("ya29.abc", 3600)).await;
        let got = cache.get_fresh("k1").await.unwrap();
        assert_eq!(got.access_token, "ya29.abc");
    }

    #[tokio::test]
    async fn get_fresh_rejects_expired_entries() {
        let cache = TokenCache::default();
        // Already expired 1 second ago.
        cache.insert("k1".to_string(), token("stale", -1)).await;
        assert!(cache.get_fresh("k1").await.is_none());
    }

    #[tokio::test]
    async fn get_fresh_rejects_entries_in_5min_buffer() {
        let cache = TokenCache::default();
        // Expires in 200s — within the 300s buffer → treat as expired.
        cache.insert("k1".to_string(), token("about-to-expire", 200)).await;
        assert!(cache.get_fresh("k1").await.is_none());
    }

    #[tokio::test]
    async fn evict_removes_single_entry() {
        let cache = TokenCache::default();
        cache.insert("a".to_string(), token("tok-a", 3600)).await;
        cache.insert("b".to_string(), token("tok-b", 3600)).await;
        cache.evict("a").await;
        assert!(cache.get_fresh("a").await.is_none());
        assert!(cache.get_fresh("b").await.is_some());
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn clear_removes_all_entries() {
        let cache = TokenCache::default();
        cache.insert("a".to_string(), token("tok-a", 3600)).await;
        cache.insert("b".to_string(), token("tok-b", 3600)).await;
        cache.clear().await;
        assert_eq!(cache.len().await, 0);
    }

    #[tokio::test]
    async fn insert_overwrites_existing_entry() {
        let cache = TokenCache::default();
        cache.insert("k".to_string(), token("old", 3600)).await;
        cache.insert("k".to_string(), token("new", 3600)).await;
        assert_eq!(cache.get_fresh("k").await.unwrap().access_token, "new");
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn concurrent_access_is_safe() {
        let cache = std::sync::Arc::new(TokenCache::default());
        let mut handles = vec![];
        for i in 0..50 {
            let c = cache.clone();
            handles.push(tokio::spawn(async move {
                c.insert(format!("k{i}"), token(&format!("v{i}"), 3600)).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(cache.len().await, 50);
        // Spot-check a few
        assert_eq!(cache.get_fresh("k0").await.unwrap().access_token, "v0");
        assert_eq!(cache.get_fresh("k49").await.unwrap().access_token, "v49");
    }
}
