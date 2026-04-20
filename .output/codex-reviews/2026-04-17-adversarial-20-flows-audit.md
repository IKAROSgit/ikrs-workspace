# Codex Adversarial Audit — 20 Critical User Flows

**Date:** 2026-04-17
**Reviewer:** Codex
**Scope:** Static analysis of 20 high-risk flows in ikrs-workspace @ commit `7a95d6a`
**Context:** After a four-bug day (TS strict-null, signInWithPopup broken in Tauri, missing client_secret, deny-all Firestore rules), hunting bugs #5-#N that compile + unit-test clean.

Gate 6 reminder: Flows 1-4, 9-11, 15 touch auth/OAuth/third-party APIs — each needs an end-to-end dry-run before next PASS. Flows 12, 14, 16 touch spawn/MCP process contracts — same requirement. None have captured HTTP traces in the repo today.

---

## Scenario 1: Sign in, close app, reopen
- **Flow:** User signs in successfully → quits Tauri app → relaunches. Expect Firebase session to persist, `consultant` doc to load, land in `AppContent`.
- **Files touched:** `src/providers/AuthProvider.tsx:77-113`, `src/lib/firebase.ts:1-17`.
- **Happy path works:** Yes, in principle — Firebase JS SDK uses IndexedDB persistence by default in a webview and `onAuthStateChanged` will rehydrate.
- **Failure mode 1:** Tauri WKWebView on macOS runs each app launch in a fresh user-data directory if the app-data bundle id ever changes. `lib.rs:13` migrates `com.moe_ikaros_ae.ikrs-workspace → ae.ikaros.workspace` once, but IndexedDB lives under WebKit's `Library/WebKit/<bundle-id>/` tree, not the app-data dir being migrated. **A bundle id rename orphans the Firebase IndexedDB session.** Moe will appear signed-out on first launch after the rename. → **Surface:** silent re-sign-in required. → **Mitigation:** none shipped.
- **Failure mode 2:** `onAuthStateChanged` callback at `AuthProvider.tsx:78` is `async` and fires `setLoading(false)` at line 110 only after the `getDoc(consultants/{uid})` round-trip. If offline at launch with a valid cached token, Firestore `getDoc` can hang until its internal timeout; the app stays on the "Loading..." screen (`AuthProvider.tsx:243-249`) with no escape hatch. → **Surface:** silent wedge. → **Mitigation:** gap — no timeout, no offline fallback, no "Skip" button.
- **Severity:** HIGH
- **Recommended action:** Wrap the `getDoc` in `Promise.race([getDoc, timeout(8s)])`; on timeout, render the app anyway with a `consultant=null` banner ("Offline — some data may be stale"). Add a Loading-screen cancel that signs out after 15s.

---

## Scenario 2: Firestore rules change underfoot mid-session
- **Flow:** Consultant signs in, consultant doc loads. Admin rotates rules so `consultants/{uid}` denies. App continues; next task write hits denied.
- **Files touched:** `AuthProvider.tsx:80-84` (read only once at sign-in — no listener); `EngagementProvider.tsx:46-74` (3 live snapshots).
- **Happy path works:** N/A (adversarial).
- **Failure mode 1:** `consultant` state is a one-shot `getDoc` (AuthProvider.tsx:82). If rules flip and a future read is needed (e.g. for `strictMcp` resolution via `engagement.settings.strictMcp`), the app still trusts the initial snapshot. Stale consultant data stays in memory indefinitely. → **Surface:** silent, no indication. → **Mitigation:** gap.
- **Failure mode 2:** `EngagementProvider` snapshot listeners (`:47, :56, :69`) will emit an error callback that is NOT subscribed — `onSnapshot` takes an optional 3rd error arg, the code passes only the success callback. A rules-change denial is silently swallowed and engagements/tasks stop updating with no user-visible event. → **Surface:** silent. → **Mitigation:** gap.
- **Severity:** HIGH
- **Recommended action:** Add the error callback to every `onSnapshot` in `EngagementProvider.tsx:47,56,69`, surface to `claudeStore.setError` or a toast. Promote `consultant` to a live snapshot listener too.

---

## Scenario 3: Sign out while sign-in in-flight
- **Flow:** User clicks "Sign in", sees the consent screen in browser, then clicks "Cancel" in the app → internal `cancelSignIn()` fires → but user has already completed the Google consent → Rust callback races cancel.
- **Files touched:** `AuthProvider.tsx:115-215`, `src-tauri/src/commands/oauth.rs:194-206`, `src-tauri/src/oauth/identity_server.rs:134-305`.
- **Happy path works:** Partially — cancel aborts the spawned task.
- **Failure mode 1:** `cancelSignIn` (`AuthProvider.tsx:212-215`) calls Rust `cancel_firebase_identity_flow` which `handle.abort()`s the task. But the Rust task may already have emitted `firebase-auth:id-token-ready` before abort. The frontend listener (`AuthProvider.tsx:149-169`) is unregistered in the `finally` block (line 204) AFTER `setSignInInFlight(false)` — but **the `finally` only fires after `idTokenPromise` settles**, which happens via `resolveRef?.(idToken)` from the event. If cancel runs before the event but abort loses the race, the promise never resolves and the listener stays alive. Meanwhile `signInInFlight` was already forced false by cancel. If the user clicks Sign In again, a NEW listener registration happens and a pending id_token from the old flow will resolve the OLD promise — whose `finally` now runs `unlistenReady()` cleaning up the new listener too. → **Surface:** silent, subsequent sign-in silently fails. → **Mitigation:** gap.
- **Failure mode 2:** `cancelSignIn` does not clear `signInError`, so a stale error from a previous attempt can persist across a cancel → retry cycle.
- **Severity:** HIGH
- **Recommended action:** Generate a `flowId` (uuid) per sign-in attempt, embed in the Rust->FE event payload, ignore events whose flowId doesn't match the current attempt. Clear `signInError` at the top of both `signIn` and `cancelSignIn`.

---

## Scenario 4: Switching Google account
- **Flow:** Sign in as A → sign out → sign in as B with a different email. Expect consultant state to fully swap.
- **Files touched:** `AuthProvider.tsx:78-113,217-220`, `EngagementProvider.tsx:46-74`.
- **Happy path works:** Most paths yes.
- **Failure mode 1:** `logOut` at `AuthProvider.tsx:217-220` calls `signOut(auth)` and `setConsultant(null)`. It does **not** reset `useEngagementStore`, `useTaskStore`, `useClaudeStore`, `useMcpStore`, or `useUiStore`. When user B signs in, `EngagementProvider` re-subscribes and overwrites engagements/clients — but stale `activeEngagementId` from user A remains (`engagementStore.ts:23`). `claudeStore.historyCache` at `claudeStore.ts:16` still contains user A's chat history keyed by engagement IDs that user B may coincidentally also own (very unlikely) or leak across if multi-consultant M3 ever merges stores. → **Surface:** silent data leak across accounts. → **Mitigation:** gap.
- **Failure mode 2:** `prompt=select_account` is set on the Firebase flow (`commands/oauth.rs:179`). But the user-engagement OAuth (`commands/oauth.rs:74`) uses `prompt=consent`, which does NOT force account picker — it defaults to the Chrome-browser most-recently-signed-in Google account, which may not be the consultant's work account. → **Surface:** user silently attaches the wrong Google account to the engagement; tokens in keychain under `ikrs:{engagementId}:google` belong to a personal Gmail. → **Mitigation:** gap.
- **Severity:** CRITICAL (privacy/data-binding)
- **Recommended action:** In `logOut`, call `useEngagementStore.setState(initialState)` for every store (add an exported `resetAll()` helper). Change engagement-OAuth flow prompt to `select_account consent` (space-separated, both allowed by Google).

---

## Scenario 5: Create engagement with duplicate client domain
- **Flow:** User creates engagement "Acme / acme.com" then another "Acme / acme.com". Both get `slug=acme-com`.
- **Files touched:** `SettingsView.tsx:67-120`, `EngagementProvider.tsx:77-94`, `src-tauri/src/skills/scaffold.rs:41-110`.
- **Happy path works:** No — the second one silently shares the vault.
- **Failure mode 1:** `SettingsView.tsx:72` derives `slug = clientDomain.replace(/\./g, "-").toLowerCase()`. Two engagements with the same domain share `vaultPath = ~/.ikrs-workspace/vaults/{slug}/`. `scaffold.rs:41-46` is idempotent ("skip if exists") so the second engagement silently reuses the first's vault content. `createClient` also happily creates a duplicate `clients/` doc because `firestore.rules:68-71` allows any authenticated user to create. → **Surface:** silent — same vault holds two engagements' notes, no warning. → **Mitigation:** gap.
- **Failure mode 2:** Two duplicate client docs, both with the same `domain` — future uniqueness queries (M3 client-admin) become ambiguous.
- **Severity:** HIGH (data collision)
- **Recommended action:** Before `createClient`, query for existing `clients` where `domain == input`; offer "attach to existing" vs "create new". Append engagement-id or suffix to vault slug to guarantee uniqueness.

---

## Scenario 6: Network drop mid-create-engagement
- **Flow:** `handleCreateEngagement` is called; client write succeeds, engagement write fails (network drops between the two).
- **Files touched:** `SettingsView.tsx:67-120`, `EngagementProvider.tsx:77-94`.
- **Happy path works:** Only when online.
- **Failure mode 1:** No transaction — `createClient` then `createEngagement` then `scaffoldEngagementSkills` run sequentially. If step 2 fails, the client doc is orphaned. Firestore offline persistence would queue the engagement write, but the `await` resolves with a local ref and step 3 (scaffold) proceeds — which means **user sees success UI with no active engagement until the queue flushes**. → **Surface:** misleading (appears successful); silent (orphaned client, no recovery). → **Mitigation:** gap.
- **Failure mode 2:** **Path bug.** `SettingsView.tsx:74` constructs `vaultPath = ${home}.ikrs-workspace/vaults/${slug}/`. `homeDir()` from `@tauri-apps/api/path` returns `/Users/moe` **without** a trailing slash. Result: `/Users/moe.ikrs-workspace/vaults/acme-com/` — a path that is not under `~/.ikrs-workspace/`. `scaffold.rs` delegates to `validate_engagement_path` (`skills/mod.rs:10-22`), which checks `resolved.starts_with(~/.ikrs-workspace/vaults)` → FAILS → returns `"Path outside allowed vault directory"`. The error is caught at `SettingsView.tsx:115-117` and only `console.error`'d (silent to user). Meanwhile the Firestore engagement doc ALREADY persisted with the broken path, and `activeEngagementId` was set to a broken engagement. → **Surface:** SILENT click-does-nothing bug. Chat view later fails because vault dir doesn't exist. → **Mitigation:** none.
- **Severity:** CRITICAL — this is the bug you asked to find. Concatenation error silently ships broken vault paths into Firestore today.
- **Recommended action:** Use Tauri `join(home, '.ikrs-workspace', 'vaults', slug)` (from `@tauri-apps/api/path`) instead of template-string concatenation. Add a try/catch that surfaces `scaffoldEngagementSkills` errors to the UI and rolls back the Firestore writes.

---

## Scenario 7: Create engagement while offline
- **Flow:** Toggle Wi-Fi off → create engagement.
- **Files touched:** same as S6.
- **Happy path works:** Partially — Firestore SDK queues writes; UI doesn't know.
- **Failure mode 1:** `useOnlineStatus` exists (`useOnlineStatus.ts`) and disables the "Connect Google" button (`SettingsView.tsx:236`) but **not** the "Create engagement" button (`SettingsView.tsx:214-219`). Offline users can click Create, Firestore queues, scaffold runs (local disk works), UI resets inputs as if complete. When the user next signs out + in before reconnection, the queue is lost. → **Surface:** silent data loss. → **Mitigation:** gap.
- **Severity:** MEDIUM
- **Recommended action:** Gate Create Engagement on `isOnline` with tooltip "Engagement creation requires internet."

---

## Scenario 8: Delete engagement while Claude session active
- **Flow:** User has Claude running for engagement A, then deletes engagement A from settings.
- **Files touched:** `EngagementProvider.tsx:101-103`, `claude/session_manager.rs:207-220`, `commands/vault.rs:61-87` (archive), keychain (`credentials.rs`).
- **Happy path works:** No — there is no lifecycle hook.
- **Failure mode 1:** `deleteEngagement` in EngagementProvider.tsx:101 just does `deleteDoc(engagements/{id})`. It does NOT: (a) kill any running Claude session (`killClaudeSession`); (b) archive/delete the vault (`archive_vault`); (c) delete the keychain token `ikrs:{engagementId}:google`; (d) unregister from session registry. Orphan process keeps running pointing at a now-deleted Firestore doc; keychain accumulates tokens forever. → **Surface:** silent. → **Mitigation:** gap — no cleanup wired.
- **Failure mode 2:** Firestore rules deletion works but tasks under `ikrs_tasks` where `engagementId == deleted.id` become orphaned. Rule `ownsEngagement()` at `firestore.rules:87-94` calls `get(engagements/{id})` which fails post-delete → task reads begin throwing PERMISSION_DENIED silently (same as S2).
- **Severity:** CRITICAL (resource leak + orphan processes + silent task-access failure)
- **Recommended action:** Replace `deleteDoc` call with a compound handler: kill session → unregister keychain → archive vault → delete engagement doc → cascade-delete tasks in a batched write.

---

## Scenario 9: Cancel OAuth mid-flow twice rapidly
- **Flow:** Click Connect Google → close the browser tab → click Connect again → click Cancel immediately.
- **Files touched:** `SettingsView.tsx:122-163`, `commands/oauth.rs:20-100`, `oauth/redirect_server.rs:48-137`.
- **Happy path works:** Partially.
- **Failure mode 1:** `OAuthState.pending_server: Mutex<Option<JoinHandle>>` allows only one handle. Second `start_oauth_flow` aborts the first (`commands/oauth.rs:32-35`). But **port binding** uses `bind_with_fallback` scanning `49152..=49162`. The aborted task may not have actually released the socket yet (abort is cooperative for blocking syscalls); the second flow scans the next free port. Now `pending_verifier` is overwritten (`commands/oauth.rs:41-43`) with the NEW verifier. If the first tab's Google callback lands on the first server (still bound during the abort race window) after the verifier overwrite, the token exchange uses the wrong verifier → Google returns `invalid_grant`. → **Surface:** token exchange error bubbles to frontend as "Token exchange error: {body}" — surfaces, but misleadingly. → **Mitigation:** partial; only one-in-flight is enforced.
- **Failure mode 2:** `redirect_server.rs` does NOT implement `state` CSRF protection or a timeout — the `listener.accept()` call at line 58-61 blocks forever if no callback ever lands. Second cancel aborts it, but any process scanning localhost ports (another dev tool, Postman, a different app) can hit `http://localhost:49152/oauth/callback?code=...` and the server will treat any request with a `code=` query as Google's callback. → **Surface:** silent token-exchange attempt against attacker-provided code (Google rejects, fail surfaces but reason is ambiguous).
- **Severity:** HIGH (redirect_server missing state + timeout) + MEDIUM (race)
- **Recommended action:** Port `identity_server.rs`'s CSRF-state + 310s accept timeout into `redirect_server.rs`. Track verifier alongside its issuing flow-id not in a shared slot.

---

## Scenario 10: OAuth succeeds but engagement doc gone
- **Flow:** While OAuth in-flight in browser, engagement is deleted from another device (M3) or the local `deleteEngagement` call fires.
- **Files touched:** `redirect_server.rs:124-131`, `SettingsView.tsx:122-163`.
- **Happy path works:** N/A (adversarial).
- **Failure mode 1:** Rust stores token in keychain under `ikrs:{engagementId}:google` regardless of whether the engagement doc still exists. Emits `oauth:token-stored`. Frontend sets `oauthStatus="success"` even though the engagement has vanished from the active list. Next spawn for that engagement uses an orphaned token; deleting the engagement later leaks the keychain entry (S8). → **Surface:** misleading success. → **Mitigation:** gap.
- **Severity:** MEDIUM (recoverable, but leaky)
- **Recommended action:** On `oauth:token-stored`, re-verify the engagement still exists before showing success; otherwise delete the keychain entry and surface "engagement no longer exists."

---

## Scenario 11: Google Workspace domain-restricted OAuth
- **Flow:** Consultant's `@blr-world.com` admin blocks third-party OAuth apps. Sign-in attempt hits `admin_policy_enforced` error.
- **Files touched:** `identity_server.rs:173-182`, `AuthProvider.tsx:276-278`.
- **Happy path works:** No — and the error UX is poor.
- **Failure mode 1:** Identity server catches `error` param and emits `firebase-auth:error` with text `"Google OAuth error: {err}"` where `{err}` is the raw Google error code like `access_denied` or `admin_policy_enforced`. Frontend renders that string verbatim (`AuthProvider.tsx:276-278`). User sees "Google OAuth error: admin_policy_enforced" with no hint of what to do. → **Surface:** surfaces but unhelpfully. → **Mitigation:** gap — no user-friendly mapping.
- **Severity:** MEDIUM
- **Recommended action:** Map known Google error codes in `AuthProvider.tsx` to actionable messages (contact IT; try a different account).

---

## Scenario 12: Spawn Claude with no network
- **Flow:** Disable Wi-Fi → start a Claude session.
- **Files touched:** `useWorkspaceSession.ts:40-99`, `claude/session_manager.rs:31-171`, `claude/auth.rs:33-51`.
- **Happy path works:** Preflight catches the online check — but not the auth check.
- **Failure mode 1:** `useWorkspaceSession.ts:41-46` checks `navigator.onLine` and bails. But the preflight `claude_version_check` and `claude_auth_status` at lines 54-67 both spawn `Command::new("claude")` (`auth.rs:7,34`) with PLAIN `"claude"`, NOT the resolved binary path from `ResolvedBinaries`. On macOS app-bundle builds, the sandbox PATH does NOT include `~/.claude/local/bin` or `/usr/local/bin` consistently. **Every Claude preflight will silently fail** on packaged builds that users run outside a shell — returning `installed: false` (line 23-27) with no diagnostic about WHY claude wasn't found. → **Surface:** misleading error ("Claude CLI not found") when binary is actually installed. → **Mitigation:** gap — only `session_manager::spawn` uses the resolved path (`:87`).
- **Failure mode 2:** After spawn succeeds, Claude CLI makes its first Anthropic API call. If that fails with a network error, Claude exits non-zero. `monitor_process` (`session_manager.rs:224-287`) emits `claude:session-crashed` with `classify_exit(exit_code)` — exit 1 → "Claude CLI error" (`:291`). The user sees a generic error, no retry, no "network unreachable" hint. → **Surface:** generic. → **Mitigation:** gap.
- **Severity:** HIGH (preflight binary-path bug is shipped-and-broken on sandboxed macOS)
- **Recommended action:** Refactor `claude/auth.rs` to take `State<ResolvedBinaries>` and use `resolved.claude` path. Parse `stderr` in `monitor_process` to extract network-specific error patterns.

---

## Scenario 13: Claude process killed externally
- **Flow:** Activity Monitor → force-quit the `claude` PID. App should detect within a few seconds.
- **Files touched:** `session_manager.rs:224-287`, `claude/registry.rs:56-70,118-128`.
- **Happy path works:** Eventually (2s poll — line 263).
- **Failure mode 1:** `monitor_process` polls `child.try_wait()` every 2 seconds. On external kill, within ~2s the monitor emits `claude:session-crashed`, removes session from map, unregisters from file registry. Good. But the frontend `useClaudeStream` (`hooks/useClaudeStream.ts:93-99`) handles `claude:session-crashed` ONLY by setting error — it does NOT reset `status` from "error" back to anything, does NOT clear sessionId. User clicks "Reconnect" — no reconnect button exists; only way is to switch engagement + switch back. → **Surface:** user-visible error, recoverable only via engagement-switch. → **Mitigation:** gap.
- **Failure mode 2:** `cleanup_orphans` (`registry.rs:118-128`) at startup kills any claude PID still alive from previous registry. This is correct, BUT it walks the registry **after** the registry was loaded — it does not check the registry's `pid` is actually the claude process spawned by THIS app. **PID reuse race:** if macOS recycles the PID to an unrelated process after app crash, `is_claude_process` mitigates it (line 89-97 checks `ps comm=` contains "claude"). Good — this is defensive. Not a bug.
- **Severity:** MEDIUM (UX — no reconnect path)
- **Recommended action:** Add a "Reconnect" button in ChatView when `status === "error"`. Reset sessionId on `session-crashed`.

---

## Scenario 14: Two engagements, rapid Claude switching — MCP token cross-contamination
- **Flow:** Engagement A has a Google token in keychain. Engagement B does not. Rapidly switch A→B→A→B.
- **Files touched:** `useWorkspaceSession.ts:101-167`, `session_manager.rs:31-171` (`max_sessions=1`), `claude/commands.rs:18-32`.
- **Happy path works:** One session at a time is enforced.
- **Failure mode 1:** `max_sessions = 1` kills the prior session when spawning the next (`session_manager.rs:43-58`). But `spawn_claude_session` writes `.mcp-config.json` into the ENGAGEMENT PATH (`mcp_config.rs:93-99`). Two engagements, two paths, two configs — so no file cross-contamination. But `GOOGLE_ACCESS_TOKEN` is passed via `Command::envs()` (`session_manager.rs:110`). The per-MCP `env` entry at `mcp_config.rs:42-44` is LITERALLY the string `"${GOOGLE_ACCESS_TOKEN}"`. **This relies on Claude CLI performing shell-style interpolation on MCP env values** — I cannot confirm from the source here that Claude CLI does this. If Claude CLI passes env values verbatim to the MCP subprocess, the MCP server sees `GOOGLE_ACCESS_TOKEN="${GOOGLE_ACCESS_TOKEN}"` (literal dollar-brace-string) and rejects every Gmail call. → **Surface:** silent — the Gmail/Drive/Calendar MCP calls start returning auth errors, which do fire `claude:mcp-auth-error`, but the ROOT cause is a misunderstanding of Claude CLI's config-substitution semantics. Prior Codex reviews (2026-04-16-m2-phase3b-final-review.md:22) asserted "Claude CLI resolves env vars in MCP config" — this is an UNVERIFIED assertion. → **Mitigation:** gap unless Claude CLI actually does interpolate. Needs a Gate-6 dry-run.
- **Failure mode 2:** If rapid-switch happens during the 2s monitor poll window, two claude processes may transiently exist; `cleanup_orphans` only runs at startup. Live double-spawn is possible if `spawn()` is called twice within ms.
- **Severity:** CRITICAL if Claude CLI does not interpolate — would explain any "Gmail returns 401" observation going forward. Needs verification TODAY.
- **Recommended action:** **Verify Claude CLI env-interpolation behavior via a throwaway test: spawn Claude with mcp-config containing `"env": {"TEST": "${FOO}"}` and `.envs([("FOO","hello")])`. If the MCP server receives `TEST=${FOO}` → this entire design is broken.** If confirmed broken, fix by resolving the token string into the mcp-config at generate-time (`mcp_config.rs`) and write an ephemeral per-spawn config file (deleted after spawn).

---

## Scenario 15: Gmail MCP auth error mid-turn
- **Flow:** Token expires during a Claude turn that calls `mcp__gmail__search`. Claude emits mcp-auth-error.
- **Files touched:** `useClaudeStream.ts:101-108`, `claudeStore.ts:149-153`.
- **Happy path works:** Partially — toast fires.
- **Failure mode 1:** `token_refresh::refresh_if_needed` runs at SPAWN TIME (`commands.rs:20-23`). It injects the refreshed token into the Claude subprocess's env. The Claude subprocess then stays alive for hours; its child MCP processes hold the SAME env `GOOGLE_ACCESS_TOKEN` forever. **There is no mid-session token refresh.** Tokens expire after 1 hour; subsequent Gmail MCP calls fail. The app will show `authError` via `setAuthError` but re-authenticating from Settings writes a new token to keychain — which does nothing for the live subprocess. Only a full session kill+respawn picks up the new token. → **Surface:** surfaces as auth error, but re-auth does NOT fix the live session. → **Mitigation:** gap.
- **Severity:** HIGH (1-hour session lifetime on any real-use flow)
- **Recommended action:** Either (a) when `setAuthError` fires, auto-kill the session and prompt re-auth + reconnect; or (b) implement a refresh mechanism that signals Claude CLI to re-read env (not available today — so (a) is the fix).

---

## Scenario 16: Obsidian MCP vault path with spaces/unicode
- **Flow:** Vault at `~/Library/CloudStorage/GoogleDrive-moe@ikaros.ae/Shared drives/99 Agent Drive/Claude - IKRS/Obsidian Vault/engagements/blr-world/` (per ADR-013).
- **Files touched:** `mcp_config.rs:73-85`, `session_manager.rs:31-171`.
- **Happy path works:** Untested.
- **Failure mode 1:** `mcp_config.rs:78-79` pushes the vault path as an arg to `npx @bitbonsai/mcpvault@1.3.0` with `vp.to_string_lossy().to_string()`. The args go into a JSON `args` array, serialized by `serde_json` → fine. Claude CLI then reconstructs the command; Node's `process.argv` will receive the spaces+unicode intact **IF** Claude CLI uses the JSON config's array form. → Probably works, but **no test covers unicode or spaces**.
- **Failure mode 2:** `validate_engagement_path` (`skills/mod.rs:10-22`) hard-codes `~/.ikrs-workspace/vaults` as the only allowed base. Once Phase 4d migrates to the Drive path, **every scaffold call will be rejected** by this validator until the allowed-base is updated. → **Surface:** "Path outside allowed vault directory" error (same as S6 failure mode 2). → **Mitigation:** gap, known but unshipped.
- **Severity:** HIGH (will break the instant Phase 4d lands)
- **Recommended action:** Parameterize `validate_engagement_path` via a config/list of allowed bases OR defer Phase 4d enforcement with a WARN review. Add integration tests for unicode/space paths.

---

## Scenario 17: Task done on two devices simultaneously
- **Flow:** Open app on Mac A + Mac B (both signed in same consultant). Both tick the same task done.
- **Files touched:** `useTasks.ts:29-39`, `EngagementProvider.tsx:112-117`.
- **Happy path works:** Last-write-wins via Firestore.
- **Failure mode 1:** `toggleStatus` in `useTasks.ts:29-39` reads `task.status` from local state, computes next, writes. Two devices: both read "todo", both write "in_progress". No lost data — both arrive at "in_progress". Second click on each → both read "in_progress" and write "done". Convergent. BUT the `toggleStatus` cycle is `todo → in_progress → done → todo`. If one device sees "in_progress" and another sees "done" due to replication delay, one might write `done → todo` while the other writes `in_progress → done`. → **Surface:** apparent "task went back to todo" flicker. → **Mitigation:** acceptable for single-consultant M2.
- **Severity:** LOW (acceptable; no data loss)
- **Recommended action:** Accept for now; revisit at M3 with proper CRDT or transaction.

---

## Scenario 18: Clock skew ± 10 minutes
- **Flow:** VM time is 10 minutes ahead/behind Google time.
- **Files touched:** `token_refresh.rs:16-20`, `identity_server.rs:147-161` (310s timeout), `AuthProvider.tsx:38,181-184` (5-minute FE timeout), `redirect_server.rs` (no timeout at all — see S9).
- **Happy path works:** 5-minute buffer saves most cases.
- **Failure mode 1:** `token_refresh.rs:17`: `expires_at <= now + 300`. If the VM clock is 10 minutes ahead of reality, a valid 1-hour token is treated as expired immediately on store; a Google `refresh_token` call happens for every spawn. Wasteful but harmless. If the VM clock is 10 minutes BEHIND reality, a token that's actually expired appears valid for up to 10min; the first Gmail call will 401 and trigger Scenario 15. → **Surface:** S15 cascade. → **Mitigation:** gap.
- **Failure mode 2:** Nonce freshness — not checked in code. Firebase validates `exp` server-side; skewed clocks could cause `signInWithCredential` to reject the id_token. User sees "Sign-in timed out" or similar generic error from Firebase.
- **Severity:** MEDIUM
- **Recommended action:** Widen the expiry buffer to 600s. Log a startup warning if `Date.now()` diverges from an HTTP Date header by more than 120s.

---

## Scenario 19: Downgrade v0.1.0 → v0.0.9
- **Flow:** User installs newer app, creates engagement (writes `_v: 1` docs), then installs older build that doesn't understand `strictMcp` or `ikrs_tasks` collection.
- **Files touched:** `AuthProvider.tsx:87` (`_v: 1`), `EngagementProvider.tsx:80,89,106` (all new docs `_v: 1`), `tauri-plugin-sql`, `lib.rs:10-40` (app-data migration is one-way rename).
- **Happy path works:** New code ignores unknown fields.
- **Failure mode 1:** `lib.rs:migrate_app_data` renames `com.moe_ikaros_ae.ikrs-workspace` → `ae.ikaros.workspace`. **This is irreversible.** A downgrade to a build that still uses the old identifier will see an empty app-data dir. SQLite DBs, session registry, preferences — all gone. The OLD install's data is now in the NEW path which the old build does not read. → **Surface:** silent data loss on downgrade. → **Mitigation:** phase4c downgrade-protection referenced in commit 45ebb3d — need to verify what it actually blocks.
- **Failure mode 2:** Tasks collection renamed `tasks → ikrs_tasks` (commit 7a95d6a). An older build that queries `tasks` with `consultantId==uid` matches Mission Control's `tasks` and gets PERMISSION_DENIED. User sees empty task list silently.
- **Severity:** HIGH
- **Recommended action:** If a downgrade protection exists, verify it refuses to launch when it sees data from a newer `_v`. Treat `tasks` collection rename as a one-way migration; document the forward-only guarantee.

---

## Scenario 20: Malicious MCP tool call — cross-engagement token read
- **Flow:** Prompt injection in an email causes the Gmail MCP (or a malicious npm package in `@shinzolabs/gmail-mcp`) to attempt to read keychain entries for OTHER engagements.
- **Files touched:** `credentials.rs:1-58`, `session_manager.rs:106-115`.
- **Happy path works:** Partial defense in depth.
- **Failure mode 1:** The MCP process inherits the Claude subprocess env, which contains ONLY `GOOGLE_ACCESS_TOKEN` for the active engagement. **However, the MCP process has the user's full filesystem access** (no sandbox) — it can read `~/Library/Keychains/login.keychain-db` indirectly through any CLI tool it decides to spawn, or read files across the entire vaults dir, including **other engagements' `.mcp-config.json`**. The Rust `validate_engagement_path` protects only the scaffold-write side, not the MCP-read side. → **Surface:** silent — user would not notice a malicious tool exfiltrating data. → **Mitigation:** `--disallowed-tools Bash` (`session_manager.rs:70-71`) blocks the obvious escape; but MCP servers have their own tool surface that bypasses this.
- **Failure mode 2:** Keychain key format `ikrs:{engagement_id}:google` (`credentials.rs:57`) is not privilege-scoped at the OS level. Any process running as the user can query `security find-generic-password -s ikrs-workspace` and enumerate all tokens. The `ikrs-workspace` service name acts as a namespace for the app, not a boundary.
- **Severity:** CRITICAL at M3 multi-tenant; MEDIUM for single-user M2
- **Recommended action:** Defer to M3. For M2, document the trust boundary: "MCP servers have your full file access; only install trusted npm packages." Consider vendoring Gmail/Calendar/Drive MCPs at pinned hashes.

---

## Summary Table

| # | Scenario | Severity | Needs action now? |
|---|---|---|---|
| 1 | Sign in persistence | HIGH | Yes — loading timeout |
| 2 | Rules change underfoot | HIGH | Yes — onSnapshot error handlers |
| 3 | Sign out during in-flight sign-in | HIGH | Yes — flowId |
| 4 | Switching Google account | CRITICAL | Yes — resetAll + prompt=select_account |
| 5 | Duplicate client domain | HIGH | Yes — uniqueness check |
| 6 | Create mid network drop | CRITICAL | **YES — path-concat bug shipped today** |
| 7 | Create offline | MEDIUM | Later |
| 8 | Delete engagement w/ active session | CRITICAL | Yes — cascade cleanup |
| 9 | Cancel OAuth twice | HIGH | Yes — port state + CSRF |
| 10 | OAuth after engagement gone | MEDIUM | Later |
| 11 | Domain-restricted OAuth | MEDIUM | Later — message map |
| 12 | Spawn Claude offline / binary-resolver drift | HIGH | **YES — auth.rs bypasses resolver** |
| 13 | Externally killed Claude | MEDIUM | Later — reconnect UX |
| 14 | MCP env-var interpolation unverified | CRITICAL | **YES — Gate-6 dry-run required TODAY** |
| 15 | Gmail auth mid-turn | HIGH | Yes — kill+respawn on auth error |
| 16 | Unicode/spaces vault path | HIGH | Yes at Phase 4d |
| 17 | Two-device task race | LOW | No — M3 |
| 18 | Clock skew | MEDIUM | Later |
| 19 | Downgrade protection | HIGH | Yes — verify phase4c blocker |
| 20 | Cross-engagement token read | CRITICAL-at-M3 | Later — document boundary |

---

## Patterns Observed

1. **Template-string path construction instead of `path.join`** — S6 is a shipped bug of exactly this shape. Any time you see `` `${home}...` `` without a separator, audit.
2. **Fire-and-forget error swallowing** — `.catch(() => {})` on cancel, `console.error` in create-engagement, `onSnapshot` with no error callback. Four of these each defeat the four-bug lesson.
3. **Unverified assertions about third-party behavior** — "Claude CLI resolves `${VAR}` in MCP config" (S14), "Firebase validates nonce" (AuthProvider comment is hedging), "Desktop OAuth doesn't need client_secret" (FIXED TODAY, same class). These are Gate-6 violations. Every one must have a recorded dry-run.
4. **One-shot reads treated as live state** — `consultant` loaded once and never refreshed (S2). Mutable state assumed immutable.
5. **Lifecycle hooks missing on delete paths** — engagement delete (S8), logOut (S4) both skip cascading cleanup.
6. **Binary-path resolution done in one place, not all places** — `auth.rs` (S12) silently bypasses the sandbox-safe resolver. Anyone adding a new `Command::new("claude")` will repeat this.

## Top 3 Must-Fix Before Moe Daily-Use

1. **S6 — `vaultPath` string concat bug (`SettingsView.tsx:74`).** This is live and will silently ship broken engagements into Firestore today. FIX FIRST: replace with `await join(home, '.ikrs-workspace', 'vaults', slug)`. Surface scaffold errors to UI.
2. **S14 — MCP env-var interpolation is unverified.** If Claude CLI doesn't interpolate `${GOOGLE_ACCESS_TOKEN}` in its MCP config, all Gmail/Calendar/Drive integration has been broken since Phase 3b shipped. Run a 5-minute smoke test TODAY: `echo '{"mcpServers":{"t":{"command":"sh","args":["-c","echo $FOO"],"env":{"FOO":"${BAR}"}}}}' > /tmp/mcp.json; BAR=hello claude --mcp-config /tmp/mcp.json --print --input-format stream-json --output-format stream-json`. Capture what the MCP process sees.
3. **S12 — `claude_version_check` / `claude_auth_status` use `Command::new("claude")` directly.** Will silently fail on packaged sandboxed macOS builds where PATH is restricted. Users will see "Claude CLI not found" when it's actually installed. Refactor `auth.rs` to take `State<ResolvedBinaries>`.

## Deferred to M3

- S7 (offline create-engagement UX polish)
- S10 (OAuth-after-engagement-gone)
- S11 (OAuth error messaging)
- S13 (reconnect UX)
- S16 (awaiting Phase 4d)
- S17 (multi-device task race)
- S18 (clock skew tolerances)
- S20 (cross-engagement token access — becomes real at multi-tenant)

## Gate 6 Findings

Flows that touch third-party auth/APIs and have no recorded end-to-end dry-run in the last 72h:

- Firebase identity PKCE with client_secret (S3, S4, S11, S18) — FIXED TODAY but no retained HTTP trace.
- Engagement OAuth (redirect_server.rs, S9, S10) — no state, no timeout, no trace.
- Claude CLI env-var interpolation (S14) — UNVERIFIED assertion, no dry-run.
- Google token refresh endpoint (S15, S18) — no captured trace.

Each of these is a standing HOLD per Gate 6 until a `tools/smoke/*-dryrun.sh` is checked in and shown to pass.
