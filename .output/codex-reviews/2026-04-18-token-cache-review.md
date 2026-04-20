# Codex Review — In-Memory Token Cache (2026-04-18)

Reviewer: Codex | Subject: `TokenCache` for macOS keychain prompt mitigation
Scope: unpushed diff in `ikrs-workspace` (token_cache.rs + token_refresh.rs + mod.rs + lib.rs)

## Pre-verdict gates

1. `cargo check` clean — PASS (per Moe)
2. `cargo test --lib oauth` 28/28 — PASS (per Moe)
3. `npx tsc --noEmit` unchanged — PASS (no TS surface)
4. CI — N/A until push
5. Spec alignment — WARN (no retrospective spec yet; governance tightening 2026-04-17 requires one)
6. Third-party dry-run — N/A (pure in-process state)

## Verdict: WARN — three must-close wire-up gaps before push

The core abstraction is sound; the gaps are all at the integration seams.

## 7-point findings

1. Structural — `oauth/token_cache.rs` placement is correct. Scoped to OAuth, owns one concern. No relocation needed.

2. Architecture — TWO keychain-write sites do NOT populate the cache:
   - `src-tauri/src/oauth/redirect_server.rs:125` writes the first-ever token after the OAuth callback. Next `refresh_if_needed` call still hits keychain → still prompts. The whole point of the cache is defeated for the first spawn after sign-in.
   - `src-tauri/src/commands/credentials.rs:15` (generic credential setter) — if this is ever used for OAuth keys it will desync. Low priority but flag.

3. Security
   - In-memory exposure: acceptable. Refresh token + client_secret deliberately excluded (good). Access token is 1-hour-lived.
   - `tokio::sync::RwLock` does NOT poison on panic (unlike `std::sync::RwLock`). Safe.
   - Doc-comment at `token_refresh.rs:46-47` claims "cache is evicted on refresh failure" — NOT IMPLEMENTED. Both `?` operators at lines 104 and 123 return early without touching cache. Either wire eviction or delete the claim. CRITICAL: the doc is lying.
   - `get_fresh` clones. A caller grabbing the token at t=(expiry - 5min - 1s) and starting a long HTTP call could 401 mid-flight. Acceptable — same race exists in the keychain-read path. No change needed.
   - logOut → `cache.clear()`: NOT WIRED. No sign-out command calls it. Frontend Firebase signOut does not propagate into Rust state. If consultant A signs out and consultant B signs in in the same app session, B's first spawn could surface A's cached token (assuming same keychain_key collision, which is unlikely but possible if engagement IDs are reused).
   - Engagement delete → `cache.evict()`: NOT WIRED. Moe already flagged this; confirmed. Matches S8 from 2026-04-17 adversarial audit.

4. Completeness — 7 unit tests cover the data structure exhaustively. Missing: integration test proving `refresh_if_needed` second call skips keychain. Recommend a minimal test using a fake keyring or an injected trait. Not blocking for this push; file as follow-up.

5. Risk register
   - False cache hit from stale cache + newer keychain: impossible — cache is in-memory only, crash clears it, and refresh writes keychain-then-cache in the same function.
   - Memory leak: bounded by distinct engagements signed in during one app lifetime. Small. clear()/evict() wiring closes this.

6. Spec alignment — Write a 1-page retrospective spec at `docs/specs/2026-04-18-token-cache.md` documenting: problem, interim nature, eviction contracts, post-Apple-Developer behaviour. Governance requires it.

7. Implementation readiness — If `TokenCache` were NOT registered via `.manage(...)`, `app.state::<TokenCache>()` would panic at runtime on first call. Confirmed registered at `lib.rs:56`. Safe.

## Must-close before push

1. Populate cache in `redirect_server.rs` after initial token write (first-spawn prompt).
2. Either implement refresh-failure eviction OR fix the misleading doc-comment at `token_refresh.rs:46-47`.
3. Wire `cache.evict(keychain_key)` into the engagement-delete path.
4. Wire `cache.clear()` into a sign-out Tauri command invoked by frontend logOut.

## Green-light

HOLD until items 1 and 2 close. Items 3 and 4 may land same-day as a follow-up commit if blocking push is painful — but they must be filed as phase todos immediately, not drift. Retrospective spec (item 6) can land within 48h per governance norms.
