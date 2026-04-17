# Security — IKAROS Workspace

> Operational security posture for the desktop app. This document covers secret handling, key storage, and rotation procedures. For architectural security design (sandbox entitlements, OAuth flow, keychain isolation) see `docs/specs/m2-phase4a-sandbox-signing-design.md`.

---

## Secret Inventory

| Secret | Purpose | Storage (production) | Storage (dev) | Rotation trigger |
|--------|---------|----------------------|---------------|------------------|
| Tauri updater private key | Signs update bundles (`.tar.gz`, `.app.tar.gz`, `.dmg`) so the running app can verify authenticity before applying | GitHub Secret `TAURI_SIGNING_PRIVATE_KEY` | `~/.tauri/ikrs-workspace.key` (perms 600) | Suspected compromise, departure of anyone with access, annual hygiene |
| Tauri updater private key password | Decrypts the private key file | GitHub Secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | N/A (current key has no password) | Same as key |
| Tauri updater **public** key | Embedded in `tauri.conf.json` → compiled into the app → used by installed clients to verify updates | `src-tauri/tauri.conf.json` line 29 (committed, must match the private key) | same | On private key rotation — requires full app re-release |
| Apple Developer ID Application cert (.p12) | Code-signs the macOS app bundle | GitHub Secret `APPLE_CERTIFICATE` (base64) + `APPLE_CERTIFICATE_PASSWORD` | Keychain on consultant's Mac | Cert expiration (annual), theft |
| Apple signing identity string | Selects which cert to use (`Developer ID Application: <Team>`) | GitHub Secret `APPLE_SIGNING_IDENTITY` | Same | Team ID change |
| Apple ID + app-specific password | Authenticates `notarytool` against Apple's notarization service | GitHub Secrets `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Same | Password rotation |
| Firebase Web SDK config | Frontend Firebase init (API key, project ID, etc.) | GitHub Secrets `VITE_FIREBASE_*` (compile-time) | `.env.local` (gitignored) | Project migration |
| Google OAuth client ID/secret | Consultant-side OAuth for connecting client Google accounts | Compiled into app (PKCE flow — client secret is public by design for installed apps) | Same | Google Cloud Console rotation |
| Per-engagement Google access/refresh tokens | MCP access to consultant's client Google accounts | OS Keychain via `keyring` crate, key format `ikrs:{engagement_id}:google` | Same | Token expiry (automatic), revocation |

---

## Updater Key Management

### Key pair

The updater uses Tauri's updater plugin, which signs with an Ed25519 keypair in the `rsign2` format (minisign-compatible file layout, but the private key file is encrypted with Tauri's own password scheme — verify with `minisign -V` is possible for public-key verification, but the private key is not itself a raw minisign secret key). The keypair was generated 2026-04-17 with:

```bash
npx tauri signer generate -w ~/.tauri/ikrs-workspace.key --ci --password ""
```

Current public key (committed at `src-tauri/tauri.conf.json:29`):

```
dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDIzNjNGRjVBM0Y4MDBEQzkKUldUSkRZQS9XdjlqSTZDVWROdGVuYncyemVxNVc1b1ZDVGlsZlc5Y2NGaklaT2dzeGdGdmkrbm0K
```

Key ID: `2363FF5A3F800DC9`.

### Required GitHub Secrets

Before cutting any `v*` tag, the following must be set in repo Settings → Secrets and variables → Actions:

- `TAURI_SIGNING_PRIVATE_KEY` — full contents of `~/.tauri/ikrs-workspace.key` (the base64 string beginning `dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5Cg…`). Paste the entire file including the leading `untrusted comment:` line.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the key password. Set to empty string for the current key (generated with `--password ""`). **The secret must be defined even when empty** — Tauri's CI uses its presence in the environment to decide whether to prompt interactively. Omitting the secret entirely causes CI to hang waiting for input, then fail.

CI enforces both: the `build` job in `.github/workflows/ci.yml` has a `Verify signing secrets on tag push` step that fails on every matrix OS if any required secret is missing, and also checks that `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is defined (even as an empty string).

CI also enforces that the pubkey is not a placeholder: tag pushes that still contain `GENERATED_PUBLIC_KEY_HERE` in `tauri.conf.json` are rejected.

### Rotating the updater key

A rotation is a **full app re-release** — all existing installs cannot verify updates signed with the new key until they upgrade through one last update signed with the **old** key.

Procedure:

1. Generate the new keypair off the production machine: `npx tauri signer generate -w ~/.tauri/ikrs-workspace-YYYYMMDD.key --password '<strong>'`
2. Using the **old** key, cut a final release `vN-transition` whose sole payload is the pubkey update: change `src-tauri/tauri.conf.json` line 29 to the new pubkey, commit, tag, let CI sign with the old key, publish.
3. Installed clients receive `vN-transition`, verify with old pubkey, install — now they hold the new pubkey.
4. Update GitHub Secrets to the new private key + password.
5. All subsequent releases sign with the new key.
6. Zeroize the old private key file: `shred -u ~/.tauri/ikrs-workspace.key`.
7. Record the rotation date and key ID in this document's "Key History" table below.

Skipping step 2 strands every existing install — they will reject every future update because the pubkey no longer matches what they have compiled in. There is no recovery path short of re-downloading a fresh installer.

### Key History

| Date | Key ID | Status | Reason | Replaced by |
|------|--------|--------|--------|-------------|
| 2026-04-17 | `2363FF5A3F800DC9` | Active | Initial generation (replaced `GENERATED_PUBLIC_KEY_HERE` placeholder) | — |

---

## Release Readiness Checklist

Before pushing any `v*` tag:

- [ ] Pubkey in `tauri.conf.json` is real (not `GENERATED_PUBLIC_KEY_HERE`) and matches `~/.tauri/ikrs-workspace.key.pub`
- [ ] `TAURI_SIGNING_PRIVATE_KEY` set in GitHub Secrets
- [ ] `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` set in GitHub Secrets (empty string if no password)
- [ ] `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` set
- [ ] `APPLE_SIGNING_IDENTITY` set to exact Developer ID Application string
- [ ] `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` set for notarization
- [ ] `VITE_FIREBASE_*` set to real production values
- [ ] `latest.json` endpoint in `tauri.conf.json:27` resolves from the public internet (either repo is public OR endpoint points to a public CDN)
- [ ] `CHANGELOG.md` updated for the tag
- [ ] Clean-machine smoke test of the built DMG — install, open, OAuth a test engagement, spawn a Claude session, verify MCP tools work

---

## Incident Response

If you suspect the updater private key was compromised:

1. **Do not** push any more releases until step 4 is complete.
2. Revoke the GitHub Secret immediately (delete, then re-add with a junk value so partial tag pushes fail loudly).
3. Draft a public disclosure. Installed users have no way to know a compromised key was used against them — the signature verification on their end would still pass for any update the attacker signed.
4. Perform a key rotation per procedure above, treating the "old" key as untrusted — i.e. the transition release must come from a clean, known-good workstation. Consider pausing updates entirely (remove `plugins.updater` endpoints from a new release) until you can distribute a trust-reset out-of-band.

If an Apple Developer cert is compromised: revoke via Apple Developer portal, regenerate, update GitHub Secrets. The next release uses the new cert; Gatekeeper on existing installs continues to accept the old cert until its expiry since Apple's revocation only blocks new notarization, not already-notarized binaries.

---

## Keychain Scope Audit (Phase 4c §7)

**Current posture:** `src-tauri/capabilities/default.json` grants `keyring:default`, which permits any webview in the app to invoke any keyring operation through the `tauri-plugin-keyring-api` bridge.

**Audit — every keychain read/write in the codebase:**

| Site | File | Operation | Key format | Why |
|------|------|-----------|------------|-----|
| OAuth token storage | `src-tauri/src/oauth/token_refresh.rs` | Read + write | `ikrs:{engagement_id}:google` (access + refresh tokens as two entries) | Per-engagement Google credentials for MCP tool calls |
| OAuth legacy read | `src-tauri/src/commands/credentials.rs` | Read | same key format | Fallback path retained from M1 before the token refresh module landed |
| Vault key derivation (future) | `src-tauri/src/commands/vault.rs` | (reserved) | `ikrs:{engagement_id}:vault` | Reserved for age-encryption vault archive key (not yet implemented) |

**Every call site is in Rust.** Grep across `src/` for direct keyring invocation from webview JS returns zero matches as of commit HEAD. The JS side talks to credentials only via well-scoped Tauri commands (`start_oauth_flow`, `refresh_token_if_needed`, `clear_engagement_credentials`) — never directly to the keyring plugin.

**Conclusion:** The `keyring:default` capability is currently over-broad but not exploited — no webview JS reaches the keyring directly. Narrowing to a custom scope (e.g. only allowing `ikrs:*:google` keys) is **safe but low-priority**. Defer to M3, where multi-consultant isolation becomes necessary (each consultant should only see their own engagements' keys).

**Phase 4d coordination:** `capabilities/default.json` will be edited in Phase 4d to re-scope the `persisted-scope` allow-list when vault paths move to the Shared Drive (ADR-013). If we ever decide to narrow `keyring:default`, fold that change into the same Phase 4d edit to avoid touching this file twice.

**Rotation note:** If the keychain is ever compromised or the app identifier changes (as it did in Phase 4a: `com.ikaros.workspace` → `ae.ikaros.workspace`), every `ikrs:*:google` entry under the old identifier is orphaned and must be cleaned up by Keychain Access manually. The Phase 4a data migration already handled this for Moe's machine; future consultants installing a fresh build after identifier-change will not see orphans.

---

## Open Risks

Tracked separately in each phase's spec risk table. Top current items:

- Apple Developer enrolment pending — no notarized build has yet been produced (Phase 4a exit criterion unmet).
- `latest.json` endpoint assumes public repo visibility; `IKAROSgit/ikrs-workspace` is currently private. First release will 404 the updater on public internet until resolved. See `docs/decisions/2026-04-17-latest-json-hosting.md` for A/B decision doc.
- `keyring:default` Tauri capability is permissive — narrowing deferred to M3 per the audit above.

See `.output/codex-reviews/` for full audit trail.
