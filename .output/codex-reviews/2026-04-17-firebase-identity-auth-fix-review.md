# Codex Review — Firebase Identity Auth Fix (system-browser PKCE)

**Commit reviewed:** `6028cd2` — `fix(auth): replace broken signInWithPopup with system-browser PKCE → signInWithCredential`
**Reviewer:** Codex (Opus 4.7, 1M)
**Date:** 2026-04-17
**Scope:** 6 files, +580 / −11. Touches M1 AuthProvider (user-facing blocker) + Rust OAuth identity server + Tauri IPC surface.

---

## Pre-verdict binding gates (CODEX.md 2026-04-17)

| # | Gate | Result | Evidence |
|---|------|--------|----------|
| 1 | `npx tsc --noEmit` | PASS | Exit 0, no output |
| 2 | `cargo check --manifest-path src-tauri/Cargo.toml` | PASS | `Finished dev profile … target(s) in 0.62s` — 11 pre-existing warnings in `src/claude/types.rs` (unrelated dead-code), 0 errors |
| 3 | `cargo test --lib oauth::identity_server` | PASS | `5 passed; 0 failed; 0 ignored` — all 5 new unit tests green |
| 4 | `npx vitest run` | PASS | `Test Files 13 passed (13) / Tests 105 passed (105)` |
| 5 | CI status | N/A | No CI config found in repo; local gates stand in |

All four actionable gates green → no automatic HOLD from gates alone. Continue to verdict.

---

## Verdict: **WARN — conditional PASS pending 2 confirmations** (score 78/100)

**Not HOLD**: none of the findings are confirmed auth-bypass or privsec-critical on the code as shipped. The code is structurally sound, the flow is correct, the gates pass, and the fix is architecturally consistent with the Phase 4a engagement pattern Moe already trusts.

**Not PASS**: there are three items Moe needs to resolve *before pulling and depending on this*:

1. **BLOCKER-CANDIDATE (Finding 6):** Google OAuth client **type** must be verified as *Desktop app*, not *Web application*. If it is "Web application", the token exchange at `https://oauth2.googleapis.com/token` will return `invalid_client` because Web clients require a `client_secret`, which the Rust code does not send (by design, correctly). The commit description states Moe created it as "Web application". This is the single highest-probability real-world failure.
2. **SECURITY GAP (Finding 5):** The `nonce` is generated, embedded in the auth URL, passed through to the event payload as `expected_nonce` — and then the frontend **drops it on the floor**. The AuthProvider listener destructures only `{ id_token }`. No JWT-level nonce validation happens. The replay protection claimed in the commit body does not exist today.
3. **GOVERNANCE (Finding 12):** No spec was written. Per the 2026-04-17 governance tightening, M1 surgical fixes still need a retroactive 1-pager SPEC/DECISION.

Everything else (findings 1, 3, 4, 7, 10, 11) is hardening, not blocking.

---

## 7-point protocol results

### 1. Structural — dual-mutex `OAuthState`
**PASS.** Separate `identity_pending_server` + `identity_pending_state` slots are the right call. The two flows cannot cross-abort: `cancel_firebase_identity_flow` only touches the identity slots; `cancel_oauth_flow` only touches the engagement slots. No shared mutex, no priority inversion, no deadlock path — both locks are acquired and released in single statements, never held across an await. The comment on the struct (`Firebase identity flow has its own slot to avoid cross-aborting`) correctly documents the intent. Good.

One note: `identity_pending_state` is written but **never read** outside `cancel_firebase_identity_flow` (where it's just cleared). It's effectively a no-op slot today. Either wire it into a check (e.g. compare against callback state in a cross-request scenario) or drop it. As-is it's dead weight that future-Moe will ask about.

### 2. Architecture — PKCE + state + nonce threat model
- **PKCE (S256):** correct. Guards against authorization-code interception on the redirect.
- **`state` (32-byte random URL-safe b64):** correct. Guards against CSRF at the redirect URL.
- **`nonce`:** generated correctly, injected into the auth URL, **not validated client-side** (see Finding 5). So the replay protection is theoretical. Google will echo the nonce in the id_token JWT, but if nobody checks the JWT's nonce claim against the issued value, an attacker who can steal an id_token from another flow could replay it. Realistically this is a narrow window (id_tokens expire in 1h, Firebase's signature check prevents forgery), but the commit claims protection that isn't wired up.
- **Attacks that remain:**
  - **Malicious local process on loopback.** Any local program can bind `127.0.0.1:49153` first, making bind fail (→ clean error, not bypass). But a local process that *proxies* the request to the real server would not be in the redirect path — Google sends the browser to `localhost:49153`, which is either Moe's server or the attacker's. If attacker binds first, they see nothing useful because the browser's Origin header + the attacker not having the verifier/state. → No exploit.
  - **Browser-extension exfiltration of auth code.** Possible in principle; PKCE neutralizes since attacker cannot mint the verifier. → No exploit.
  - **IPC-level abuse:** the Tauri command `start_firebase_identity_flow` is invokable from any renderer code. If any untrusted JS runs in the webview (e.g. embedded iframe, extension, future MCP UI), it could kick off sign-in flows. Low severity (worst case: user sees browser popup), but worth noting as hardening: gate behind a capability check or rate-limit.
- **id_token not persisted server-side / Firebase IndexedDB session:** **Adequate for Tauri.** Firebase Auth's IndexedDB persistence works in Tauri's WebView (v2+ with `webKitPersistentStorage` enabled, which Tauri defaults to). Session persists across relaunch. The refresh-token rotation is handled by Firebase. **Caveat:** if the user's OS / Tauri webview clears IndexedDB (permission changes, app reinstall, profile deletion), the session is lost and the user must re-auth. Acceptable UX.
- **"Web application" vs "Desktop app" client:** See Finding 6 — this is the likely failure.

### 3. Security — deep walk
See **Security findings** section below. The listener-reference pattern is subtly fragile but not currently exploitable. TOCTOU check-then-exchange is clean. `fail()` closure cannot double-emit on the current paths. Port-scan range is adequate for now.

### 4. Adversarial input completeness
- **Malformed callback `?state=%ZZ`:** `urlencoding::decode` returns empty string via `unwrap_or_default()`. Empty ≠ 43-char random expected_state → rejected. Safe-by-accident; see Finding 4.
- **Replayed state within 5-min window:** the Rust task's `expected_state` is consumed by the single `listener.accept().await`. Second TCP connection never happens on this listener. → No replay.
- **Google responds twice (retry):** Google does not retry 302 redirects; the browser follows one. The server accepts exactly one connection then the task ends. → No double-emit.
- **Callback with `code` + `state` + `error` all present:** error branch wins (line 149 runs first). Emits ONE `firebase-auth:error`. Browser sees 400 error page. → Clean.
- **Malformed HTTP (not GET line):** `split_whitespace().nth(1)` returns None → empty params → missing-state error path → one error emit. → Clean.

### 5. Risk
- **User closes tab mid-flow:** Rust server is blocked on `listener.accept().await`. No timeout in Rust. Task leaks until: (a) 5-min frontend timeout fires → `cancelFirebaseIdentityFlow` aborts the task, or (b) app quits. Acceptable but the Rust side should also carry a `tokio::time::timeout(accept)` as defense-in-depth. Without it, if the frontend timeout setTimeout is suspended (tab hidden, system sleep), the Rust listener persists until app exit.
- **`openUrl()` fails (no default browser / corp policy):** `openUrl` rejects → caught by try/catch → `signInError` surfaced → server aborted via cleanup. Good.
- **Clock skew on id_token `exp`:** if >~50min between Google-issuing-token and Firebase-validating, signature check fails → `signInWithCredential` rejects → error surfaced. Rare; acceptable.
- **Firebase rejects credential (domain not authorized):** `signInWithCredential` throws → caught → error surfaced. Good — but Moe should add the domain in Firebase console proactively for both `ikaros-portal` and any future `ikrs-workspace-dev` project.

### 6. Spec/code alignment
No spec exists for this change. The 2026-04-17 governance update requires at minimum a SPEC/DECISION 1-pager even for surgical M1 fixes. Code is **above** spec because no spec exists to compare against. **Retroactive spec recommended**, not blocking.

### 7. Implementation readiness + future-proofing
- **M3 client-portal (web):** The current implementation is Tauri-only — it uses `@tauri-apps/plugin-opener` and a local loopback server. For a web portal, the correct path is Firebase's standard `signInWithRedirect` (no local server needed — web browsers expose the URL bar). Zero code reuse. This is **fine** because the two deployment modes have fundamentally different trust models — but document that split now in a CLAUDE.md/ARCH note so future-Moe doesn't try to cram both into one flow.
- **Microsoft / Apple OIDC:** Current architecture is Google-hardcoded: the auth URL, token endpoint, and scopes are inlined. To extend: refactor `identity_server.rs` to take an `OidcProvider` struct carrying `{ auth_endpoint, token_endpoint, scopes, issuer }`. Modest refactor (~30 LOC). **Not needed today** but trivial when needed. Suggest leaving a TODO comment.
- **`ikrs-workspace-dev` Firebase project migration:** All Firebase config flows through `src/lib/firebase.ts` (not touched in this commit). Swapping projects is an env-var change. The OAuth client_id is a separate `VITE_GOOGLE_OAUTH_CLIENT_ID` env — also swappable. **No hardcoded project assumptions in this commit.** Good decoupling.

---

## Security findings (dedicated)

| # | Severity | Finding | Action |
|---|----------|---------|--------|
| 1 | MEDIUM | Listener registration precedes `resolveRef`/`rejectRef` declaration (lines 108-123 of AuthProvider.tsx). Hot-path ordering brittle — any future refactor that moves listener registration after `startFirebaseIdentityFlow()` will race and drop events. | Hoist `let resolveRef …; let rejectRef …;` **above** the two `listen()` calls. Five-line fix. |
| 2 | MEDIUM | Cosmetic UX: success HTML is written to the browser *before* token exchange. If token exchange fails, browser shows "Signed in" while app shows error. | Either move `write 200` to after token-exchange success, or change success-card wording to "Processing…". |
| 3 | HIGH | **Nonce not validated.** Rust generates + sends nonce, Rust forwards `expected_nonce` in the event payload, AuthProvider ignores it. `signInWithCredential` does not automatically cross-check nonce against an externally-issued value. Replay protection claimed in commit body does not exist. | In AuthProvider, decode the JWT payload (base64 split), assert `claims.nonce === expected_nonce`, throw on mismatch. ~15 LOC. |
| 4 | LOW | `urlencoding::decode(v).unwrap_or_default()` silently returns empty on malformed percent-encoding. Currently safe because expected_state is non-empty, but safe-by-accident. | Assert `!expected_state.is_empty()` at function entry of `start_identity_redirect_server`. Or use `subtle::ConstantTimeEq` for state comparison. |
| 5 | MEDIUM | **Likely token-exchange failure** with Google "Web application" client type. Web clients require `client_secret`; Rust code intentionally does not send one. Desktop/Native clients accept loopback+PKCE without secret. | Verify in GCP Console: `APIs & Services → Credentials → [client]`. If "Web application", create a new **Desktop app** client and swap the client_id. Do **not** add client_secret to the Rust code (would embed it in the binary). |
| 6 | LOW | No Rust-side timeout on `listener.accept()`. If frontend timeout is suspended (tab hidden, sleep), server task persists. | Wrap `listener.accept()` in `tokio::time::timeout(Duration::from_secs(310), …)` — 310s = slightly > frontend 5-min. |
| 7 | LOW | Port ranges overlap: engagement 49152-49162, identity 49153-49163. Both flows concurrently + a third consumer could exhaust. | Consider shifting identity to 49163-49173 for full separation. Not urgent. |
| 8 | LOW | `start_firebase_identity_flow` IPC is callable from any renderer JS, including future untrusted iframes. | Add a capability check if M3 ever loads remote content in the webview. Not needed today. |
| 9 | INFO | `VITE_GOOGLE_OAUTH_CLIENT_ID` is baked into the client bundle — OK for OIDC public clients, **do not** add any `VITE_GOOGLE_OAUTH_CLIENT_SECRET` env. | Ensure `.env.local` has no `_SECRET` with `VITE_` prefix. |
| 10 | INFO | `identity_pending_state` slot is written but never read/compared. Dead weight. | Either wire into callback validation or remove. |

---

## Future-proofing notes

- **Second-portal (web) split:** document now that the Tauri flow and any web-portal Firebase flow are architecturally separate. The web portal will use `signInWithRedirect` or `signInWithPopup` (which works in real browsers). Do not attempt to unify them.
- **Multi-provider extension:** to add Microsoft/Apple, refactor `identity_server.rs` around a provider struct. Currently ~30-LOC refactor. Good bones; just Google-hardcoded for now.
- **Firebase project swap:** no blockers. Env-var driven.
- **Refresh-token rotation:** Firebase SDK handles this in IndexedDB. Nothing to build.

---

## Decision for Moe

**Do not pull and depend on this until you have (a) confirmed the GCP OAuth client is "Desktop app" type and not "Web application" (if Web, create a new Desktop client and swap the ID), and (b) added nonce validation in `AuthProvider.tsx` — until then the claimed replay protection is absent.** Once those two are resolved, this is a clean PASS; everything else is polish.

— Codex
