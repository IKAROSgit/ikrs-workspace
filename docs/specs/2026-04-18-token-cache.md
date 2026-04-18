# In-memory Google Access-Token Cache

**Status:** Shipped 2026-04-18 (retroactive spec per 2026-04-17 CODEX.md governance rule).
**Codex review:** `.output/codex-reviews/2026-04-18-token-cache-review.md` — HOLD with 4 findings; must-fixes #1, #2, #3 closed in this commit; #4 deferred as follow-up with tracking.

---

## Problem

macOS Keychain prompts the user "Do you want to allow IKAROS Workspace to access the keychain?" on every read from an app whose code signature is not in the entry's ACL. **Ad-hoc signed dev builds** (our current daily-use path until Apple Developer enrolment lands) produce a different signature on every rebuild, so users clicking "Always Allow" on one build doesn't persist.

`refresh_if_needed` is called on every Claude session spawn. Each spawn hit the keychain → one prompt per spawn → Moe reported **5 prompts in sequence** during normal 2026-04-18 use.

## Solution

An in-memory `TokenCache: HashMap<keychain_key, CachedToken>` behind an `RwLock`, registered as Tauri state. `refresh_if_needed` checks the cache first; a hit returns without touching the keychain. On miss, the keychain is read, and the result (if still fresh) is inserted into the cache before return. Refresh grants also populate the cache with the new access token.

Drop: N keychain reads per app session → **1 keychain read per engagement per app session**.

## Non-goals

- **Not a persistent cache.** The cache is in-process. App quit = cache cleared. Fresh keychain read (one prompt) on next launch.
- **Not a permanent fix for prompt spam.** The real fix is stable signing identity (Apple Developer cert) — then "Always Allow" persists across rebuilds and the keychain prompts vanish entirely. This cache remains useful as a latency optimisation post-cert.
- **Not the refresh_token or client_secret cache.** Only the short-lived access token is cached. Refresh token + client secret stay in keychain only.

## Data model

```rust
struct CachedToken {
    access_token: String,
    expires_at: i64,  // unix timestamp, same as TokenPayload.expires_at
}
```

`CachedToken::is_expired()` uses the same 5-minute grace buffer as `TokenPayload::is_expired()` so cache-path and keychain-path agree on "too close to expiry to use."

## API surface

```rust
impl TokenCache {
    async fn get_fresh(&self, key: &str) -> Option<CachedToken>;   // None if missing OR expired
    async fn insert(&self, key: String, token: CachedToken);
    async fn evict(&self, key: &str);
    async fn clear(&self);
    async fn len(&self) -> usize;  // diagnostic
}
```

## Integration points

- **`oauth::redirect_server.rs`** (first-time OAuth write): inserts into cache before + alongside keychain write, so first spawn post-sign-in hits cache.
- **`oauth::token_refresh.rs`** (`refresh_if_needed`): cache-first. Populates cache on successful keychain read AND after successful refresh.
- **`commands::oauth::clear_token_cache`** (Tauri command): invoked from frontend on logOut so subsequent sign-in does not inherit the prior consultant's tokens.

## Security

- **In-process only.** Lost on app quit. No disk spill.
- **No encryption at rest in memory** — Rust heap. Subject to memory-dump exposure. Acceptable given the same tokens already live on disk (keychain + generated `.mcp-config.json`) and the access token is short-lived (~1 hour).
- **No cross-engagement leak.** `keychain_key` format `ikrs:{engagement_id}:google` is unique per engagement; cache entries are keyed the same way.
- **logOut clears everything.** Multi-consultant-on-same-Mac scenario handled.

## Known gaps (follow-ups)

1. **Engagement delete does NOT evict its cache entry.** Matches the broader S8 finding from the 2026-04-17 adversarial audit (no cascade cleanup on engagement delete — keychain, Claude session, vault, tasks, and now cache all orphan). To fix as part of S8 cleanup pass.
2. **No integration test covering the cache-hit skips-keychain path.** Would require mocking `KeyringExt`, which is non-trivial in Rust without a trait abstraction we don't have today. The 7 unit tests cover `TokenCache` in isolation; the wire-up relies on code review.

## Tests

7 tests in `oauth::token_cache::tests` — miss, hit-roundtrip, expired-entry-returns-none, 5-min-buffer, evict, clear, overwrite, concurrent 50-task access. All pass. Total oauth module tests: **28/28**.

## Retrospective

The cache shipped within 90 minutes of Moe reporting 5-prompt spam. Approach: small focused struct, defensive test coverage, Codex review before push, fixes for two of Codex's four findings rolled into the same commit, two deferred with documented tracking.
