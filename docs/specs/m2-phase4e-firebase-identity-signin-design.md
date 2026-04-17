# M2 Phase 4e: Firebase Identity Sign-In via System-Browser PKCE

**Status:** Retroactive (shipped in commits `6028cd2` + `<hardening>` on 2026-04-17).
**Date:** 2026-04-17
**Trigger:** Daily-use blocker surfaced during Moe's first Mac launch — `signInWithPopup` (M1 AuthProvider) silently fails inside Tauri WebView.
**Scope:** 1-page bugfix spec, filed retroactively per the 2026-04-17 CODEX.md governance rule ("Any code-shipping work needs a spec, even surgical bugfixes").

---

## Problem

`src/providers/AuthProvider.tsx:67` in M1 used:

```tsx
const signIn = async () => {
  await signInWithPopup(auth, googleProvider);
};
```

`signInWithPopup` calls `window.open(...)` expecting a browser popup with a visible URL bar. Tauri WebView does not expose URL bars in popups. Result: popup opens blank, the `await` hangs indefinitely, clicking the button produces no visible effect. Moe hit this on the first real Mac launch of the app 2026-04-17 after completing the full build + ad-hoc sign + install pipeline.

## Goal

Replace the broken popup flow with a system-browser PKCE + OIDC flow that returns a Google-issued `id_token`, passed to Firebase's `signInWithCredential` to establish the session. Match the security posture Phase 4a already set for per-engagement OAuth (PKCE + loopback redirect), plus the additions needed for identity (`state` CSRF parameter, `nonce` replay protection).

## Design

### Rust backend (new module `src-tauri/src/oauth/identity_server.rs`)

- Binds a one-shot `TcpListener` on `127.0.0.1:49153` (with port fallback up to `49163`).
- Wraps `listener.accept()` in `tokio::time::timeout(310 s)` so the task cannot leak if the frontend timeout suspends.
- Parses Google's redirect callback, extracting `code`, `state`, `error`.
- Validates `state` against the value issued at flow start; mismatch → emits `firebase-auth:error` + 400 response.
- Exchanges authorization code for tokens at `https://oauth2.googleapis.com/token` with PKCE verifier (no `client_secret` — GCP OAuth client must be created as type **Desktop app**, NOT Web application).
- Emits `firebase-auth:id-token-ready` with `{ id_token, expected_nonce }` on success.
- Emits `firebase-auth:error` with `{ reason }` on any failure.

### Rust commands (`src-tauri/src/commands/oauth.rs`)

- `start_firebase_identity_flow(client_id, redirect_port) -> { auth_url, port }` — issues random `state` + `nonce`, starts the redirect server, builds the auth URL with OIDC scopes (`openid email profile`).
- `cancel_firebase_identity_flow()` — aborts the listening task if present.
- Flow state lives in a dedicated slot on `OAuthState` so it cannot cross-abort the engagement flow (port 49152) running concurrently.

### TypeScript frontend (`src/providers/AuthProvider.tsx`, `src/lib/tauri-commands.ts`)

- `AuthGate` sign-in button triggers `startFirebaseIdentityFlow` backend command.
- Registers Tauri event listeners for `firebase-auth:id-token-ready` + `firebase-auth:error` BEFORE calling the backend (no-race guarantee).
- Decodes the returned id_token JWT payload in-app to compare `claims.nonce === expected_nonce`; mismatch throws "possible replay" error. Firebase itself validates signature/iss/aud/exp when we pass the credential.
- Opens `auth_url` in the system default browser via `@tauri-apps/plugin-opener`.
- `await signInWithCredential(auth, GoogleAuthProvider.credential(id_token))` establishes the Firebase session; `onAuthStateChanged` flips the UI past `AuthGate`.
- 5-minute frontend timeout; in-flight state + Cancel button exposed to the user.

## Security Properties

| Property | Mechanism | Status |
|----------|-----------|--------|
| PKCE prevents code interception | `code_challenge=S256` over random 32-byte verifier | ✅ reused from Phase 4a `pkce.rs` |
| CSRF protection | Random 32-byte `state` issued, validated on callback | ✅ new in 4e |
| Replay protection | Random 32-byte `nonce` in auth URL + explicit equality check in AuthProvider against id_token claim | ✅ new in 4e (Codex HIGH-2 close 2026-04-17) |
| No secret in binary | Desktop-app OAuth client, no `client_secret` at token exchange | ✅ (Codex HIGH-1 close 2026-04-17) |
| Loopback-only binding | `127.0.0.1:…` — not accessible from LAN | ✅ reused pattern |
| Listener leak prevention | `tokio::time::timeout(310 s)` + frontend Cancel | ✅ |
| Session persistence | Firebase IndexedDB inside the Tauri webview | ✅ (default Firebase behavior) |
| id_token never persisted by us | Event-delivered, consumed once, dropped | ✅ |

## GCP Configuration Required

- **OAuth client type:** Desktop app (NOT Web application — Web requires a client_secret that we do not send).
- **OAuth consent screen:**
  - User Type: External (to allow `@blr-world.com`, `@gmail.com`, etc.; Internal restricts to `@ikaros.ae` only).
  - Publishing status: In production (or tester email added during Testing state).
- **Scopes requested:** `openid email profile` only. No Gmail / Calendar / Drive here — those are the engagement flow, separate OAuth.
- **Client ID:** injected at build time via `VITE_GOOGLE_OAUTH_CLIENT_ID` in `.env.local`.

## Tests

- Rust unit: 5 new tests in `identity_server::tests` covering `parse_callback` for code-only, error, url-encoded, and no-query variants, plus uniqueness + length of `generate_random_b64`. `cargo test --lib oauth::identity_server` → 5/5 pass.
- Frontend: no new vitest tests (AuthProvider has no test file in the repo today; this is tracked as follow-up in the Codex sign-off review but not blocking).
- Manual: sign in with a Google account, land past `AuthGate` into the main shell. Validated by Moe 2026-04-17 on his Mac (BLR account, ikaros-portal Firebase).

## Risks (and mitigations shipped)

| Risk | Mitigation |
|------|------------|
| User closes browser mid-flow | 5-minute frontend timeout + backend `accept()` timeout; Cancel button in-app |
| Google returns `error=access_denied` | Parsed + surfaced as Tauri event → in-app error message |
| System has no default browser | `openUrl()` error caught in frontend try/catch; surfaced as `signInError` |
| id_token expires between Google issue + Firebase consumption (clock skew) | Firebase `signInWithCredential` re-validates exp; error surfaced |
| Firebase rejects credential (e.g. domain not in Authorized Domains) | Frontend try/catch around `signInWithCredential` |
| Replay of a captured id_token | Random per-flow nonce + claim equality check |

## Future-proofing

- **M3 client portal (separate web app at `clients.ikaros.ae`):** can reuse the same Firebase project + same OAuth client ID, just with web-style `signInWithPopup` which works fine in regular browsers. No architectural change to the auth model.
- **Microsoft / Apple / other OIDC providers:** the identity_server module is provider-agnostic at the core — the only Google-specific parts are the auth URL and token endpoint URL. A future `start_microsoft_identity_flow` command would clone the Rust module with those two URLs swapped.
- **Migration to a dedicated `ikrs-workspace-dev` Firebase project later:** no code change; only `.env.local` flips + the OAuth client moves to that project.

## Retrospective

This spec did not exist before the code landed. The CODEX.md governance rule (added 2026-04-17) says all code-shipping work needs a spec — this file is the retroactive fulfilment. The code itself was Codex-reviewed in `.output/codex-reviews/2026-04-17-firebase-identity-auth-fix-review.md` which surfaced two HIGH-severity findings (wrong OAuth client type, missing nonce validation) both closed in the hardening commit on the same day.
