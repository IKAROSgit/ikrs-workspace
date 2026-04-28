# M3 Phase F — Firestore-Synced Per-Engagement OAuth Tokens

**Status:** LOCKED — pre-code challenge passed, showstopper fixes applied
**Depends on:** Phase E (dd44ee1, deployed on elara-vm)
**Branch:** `phase-f-token-sync`

## Problem

Phase E's Tier II heartbeat reads a single static
`/etc/ikrs-heartbeat/google-token.json`. The Tauri app already does
per-engagement Google OAuth correctly (tokens in Mac keychain keyed by
`ikrs:{engagement_id}:google`). The heartbeat cannot see those tokens, so it
can only monitor one inbox — currently the wrong one (personal IKAROS noise
instead of the BLR work email).

The heartbeat has no path to multi-engagement support: there is a single
`engagement_id` in heartbeat.toml and a single token file on disk.

## Architecture Decision: Firestore-Synced Encrypted Tokens

Tauri writes encrypted tokens to Firestore on OAuth success. The heartbeat
(Admin SDK, bypasses security rules) reads and decrypts them per-engagement.
Refresh-token rotation writes back to Firestore. Multi-engagement support
falls out naturally.

### Where tokens live

```
engagements/{eid}/google_tokens/google
```

Document schema:

```typescript
{
  // AES-256-GCM encrypted blob, base64-encoded.
  // Contains ciphertext || 16-byte GCM authentication tag (concatenated).
  // Both WebCrypto (TS) and Python cryptography's AESGCM produce this
  // format by default. Implementations MUST NOT separate the auth tag
  // into a different field or strip it.
  // Plaintext is JSON: { access_token, refresh_token, expires_at, client_id, client_secret }
  ciphertext: string;

  // 12-byte IV, base64-encoded. Fresh per write (never reuse IVs).
  iv: string;

  // Identifies which key was used. Allows key rotation without
  // breaking reads: reader tries current key, falls back to prior.
  // See "Key rotation config" section below.
  keyVersion: number;

  // Firestore server timestamp (FieldValue.serverTimestamp() on write).
  // Avoids clock-skew issues between Mac and VM. Lets the heartbeat
  // skip decryption if the doc hasn't changed since last read.
  updatedAt: Timestamp;

  // Which side wrote last: "tauri" | "heartbeat". Audit trail.
  writtenBy: "tauri" | "heartbeat";
}
```

#### Cross-platform test vector

To verify TS ↔ Python interop, both implementations must pass this vector:

```
Key (base64):   k5PwCv8jGkKbRMiZt4N5VmQF9GDwxJZKtqX/rF8dHYQ=
Key (hex):      939 3f00aff231a429b44c899b7837956640ff460f0c4964ab6a5ffac5f1d1d84
IV (base64):    AAAAAAAAAAAAAAAA
Plaintext:      {"access_token":"ya29.test","refresh_token":"1//test","expires_at":1700000000,"client_id":"cid","client_secret":"csec"}
Expected ciphertext+tag (base64): (generated during F.2 implementation, pasted here, both sides must match)
```

### Encryption: AES-256-GCM with operator-supplied key

#### Tradeoff table

| Criterion | AES-GCM + operator key | Cloud KMS |
|---|---|---|
| **Setup complexity** | One `openssl rand` during install | Create keyring + key in GCP Console, grant IAM roles, configure SDK |
| **Cost** | $0 | ~$0.06/10k ops — negligible, but billing must be enabled |
| **Latency** | ~0ms (local crypto) | 50-200ms per encrypt/decrypt (network round-trip to KMS) |
| **Key management** | Operator stores 32-byte key in secrets.env on VM + .env.local on Mac | GCP manages key lifecycle; operator manages IAM |
| **Rotation** | Manual: generate new key, re-encrypt, bump keyVersion | Built-in via KMS key versions |
| **Security ceiling** | Key compromise = all tokens exposed. Key lives on two machines (Mac + VM) | Key never leaves GCP HSM. Compromise requires GCP IAM breach |
| **Offline/air-gapped** | Works anywhere | Requires GCP API access for every operation |
| **Dependency footprint** | `WebCrypto` (TS) + `cryptography` (Python) — already available | `@google-cloud/kms` (TS) + `google-cloud-kms` (Python) — new deps |

**Decision: AES-256-GCM with operator-supplied key.**

Rationale: The tokens being encrypted are Google OAuth refresh tokens, which
are already revocable and have limited blast radius (read-only Calendar +
Gmail scopes). Cloud KMS adds latency on every tick (heartbeat decrypts
hourly), a new GCP dependency, and setup friction — disproportionate for the
threat model. The operator key is 32 bytes of cryptographic randomness stored
at the same security level as the Firebase service-account JSON (which
already grants full Firestore access). If the key leaks, the attacker could
also read Firestore directly via the service account, so KMS adds no
practical defense-in-depth for this deployment.

Key rotation is supported via `keyVersion`: writer bumps the version,
reader tries current key first, falls back to N-1.

### Key distribution

- **Mac (Tauri app):** `VITE_TOKEN_ENCRYPTION_KEY` in `.env.local`
  (already gitignored, same pattern as `VITE_GOOGLE_OAUTH_CLIENT_SECRET`)
- **VM (heartbeat):** `TOKEN_ENCRYPTION_KEY` in
  `/etc/ikrs-heartbeat/secrets.env` (mode 0600, same file as Gemini key)
- **Generated during install:** `openssl rand -base64 32` → operator copies
  to both locations. `install.sh` generates it if not already present and
  prints the value for the operator to paste into `.env.local`.
- **Key encoding:** The env var contains a **base64-encoded** 32-byte key
  (44 characters). Both TS and Python sides MUST base64-decode the value to
  obtain the raw 32-byte AES key before passing it to the crypto API.
  Using the base64 string directly as the key will fail (44 bytes != 32 bytes).
- **Key recovery:** If the Mac is reinstalled, `.env.local` is lost. The VM
  still has the key in `secrets.env` — operator copies it back. If both are
  lost, all Firestore tokens are unreadable and every engagement must
  re-authenticate. **Recommendation:** operator backs up the encryption key to
  a password manager or secure note.

### Key rotation config

For key rotation, the env supports two variables:

```bash
# Current key (required). Used for all new writes.
TOKEN_ENCRYPTION_KEY="<base64-encoded-32-bytes>"
TOKEN_ENCRYPTION_KEY_VERSION=1

# Previous key (optional). Used only for reading docs encrypted with the prior version.
TOKEN_ENCRYPTION_KEY_PREV="<base64-encoded-32-bytes>"
TOKEN_ENCRYPTION_KEY_PREV_VERSION=0
```

**Read logic:** Check `keyVersion` on the Firestore doc. If it matches
`TOKEN_ENCRYPTION_KEY_VERSION`, decrypt with the current key. If it matches
`TOKEN_ENCRYPTION_KEY_PREV_VERSION`, decrypt with the previous key. If
neither matches, emit error code `key_version_unknown` — operator must
update the key on that machine.

**Write logic:** Always encrypt with the current key and set
`keyVersion = TOKEN_ENCRYPTION_KEY_VERSION`.

**Rotation procedure:**
1. Generate new key: `openssl rand -base64 32`
2. Move current key to `_PREV` on both machines
3. Set new key as current, bump version number
4. Next heartbeat tick reads with old key, writes with new key — automatic re-encryption
5. After one full cycle (all engagements ticked), remove `_PREV`

## Multi-engagement TOML shape

Current `heartbeat.toml` has flat `engagement_id` and `vault_root` at top
level. Phase F replaces them with a `[[engagements]]` array:

```toml
tenant_id = "moe-ikaros-ae"

# Legacy single-engagement fields are still accepted for backwards compat
# during migration. If [[engagements]] is present, these are ignored.
# engagement_id = "blr-world"
# vault_root = "/home/ikrs/vaults/blr-world"

[[engagements]]
id = "5L12siRpQDDXnPCk892H"
vault_root = "/home/ikrs/vaults/blr-world"

# Future: add more engagements
# [[engagements]]
# id = "another-engagement-uid"
# vault_root = "/home/ikrs/vaults/another-client"

prompt_version = "tick_prompt.v1"

[llm]
provider = "gemini"
model = "gemini-2.5-pro"
temperature = 0.2
max_output_tokens = 4096

[signals]
calendar_enabled = true
gmail_enabled = true
vault_enabled = true
calendar_lookahead_hours = 24
gmail_lookback_hours = 24

[outputs]
firestore_enabled = true
telegram_enabled = true
audit_enabled = true
firestore_project_id = "ikaros-portal"
```

The tick orchestrator iterates `[[engagements]]`, and for each:
1. Reads encrypted token from `engagements/{eid}/google_tokens/google`
2. Decrypts with operator key
3. Uses the resulting credentials for Gmail/Calendar collectors
4. Refreshes if expired, writes back encrypted to Firestore
5. Runs the existing signal → LLM → output pipeline scoped to that engagement

## Refresh-token writeback flow

```
Tauri OAuth success
  → encrypt(TokenPayload) with operator key
  → write to engagements/{eid}/google_tokens/google
  → keyVersion=1, writtenBy="tauri"

Heartbeat tick (per engagement):
  → read engagements/{eid}/google_tokens/google
  → decrypt with operator key (try keyVersion N, fallback N-1)
  → if access_token expired:
      → refresh via Google token endpoint
      → encrypt new TokenPayload
      → write back to Firestore (writtenBy="heartbeat")
  → use valid access_token for Gmail/Calendar collectors
```

### Refresh-token rotation handling

**Critical:** Google may return a NEW refresh_token when refreshing an
access token (Desktop-app OAuth clients are being enrolled in refresh-token
rotation since 2022). When rotation is active, the old refresh_token is
invalidated after a grace period.

**Both sides MUST:**
1. Check `json["refresh_token"]` in every refresh response
2. If present, use the NEW refresh_token (not the old one)
3. Write the updated payload to Firestore immediately

**Existing Tauri bug (pre-Phase-F):** `token_refresh.rs:116` discards new
refresh tokens: `refresh_token: payload.refresh_token`. Phase F.2 must also
fix this: use `json["refresh_token"].as_str().unwrap_or(&payload.refresh_token)`.

**Race condition:** Tauri and heartbeat could both refresh simultaneously.
- Both get valid access tokens from Google
- If Google rotates the refresh_token, both get different new refresh tokens
- Last-writer-wins applies: the `updatedAt` (server timestamp) determines
  which write is newer
- The loser's refresh_token is orphaned, but Google's grace period (typically
  hours) means the winner's token is still valid on the next tick
- Worst case: if the loser's stale token overwrites the winner's, the next
  refresh attempt fails → heartbeat logs `oauth_refresh_failed` →
  re-auth via Tauri app. This is a rare edge (requires exact simultaneous
  refresh during a rotation event) with a clean recovery path.
- **Mitigation:** The heartbeat reads the doc BEFORE refreshing. After
  refreshing, it re-reads the doc; if `updatedAt` changed (Tauri wrote
  in between), it discards its own refresh result and uses Tauri's
  newer token. This is a simple optimistic-concurrency check.

## Firestore rules for the new subcollection

The subcollection rule MUST be nested inside the existing
`match /engagements/{engagementId}` block (Firestore rules do not cascade
to subcollections). Each client SDK read/write costs 1 `get()` call for
the ownership check — acceptable even with N engagements since each is a
separate request.

```
// Inside the existing match /engagements/{engagementId} block:

  // Per-engagement encrypted OAuth tokens. Written by Tauri (client SDK,
  // owning consultant) and heartbeat (Admin SDK, bypasses rules).
  match /google_tokens/{provider} {
    allow read, write: if request.auth != null
      && get(/databases/$(database)/documents/engagements/$(engagementId))
           .data.consultantId == request.auth.uid;
  }
```

The heartbeat uses Admin SDK which bypasses all rules. These rules only gate
client SDK access (Tauri frontend writes via Firebase JS SDK).

**Pre-deployment requirement:** The `consultantId` field on the BLR engagement
document (`5L12siRpQDDXnPCk892H`) MUST match Moe's current Firebase Auth UID
(`yenifG1QiwVZtgNo42zaoSCPRTx1`). If stale (e.g., old value
`BBxieDr3PqNn6hXQNbVuMirBdDF2`), update it before coding Phase F. Otherwise
all client SDK writes to the subcollection will be denied.

## Migration from single-token deployments

For operators already running Phase E with `/etc/ikrs-heartbeat/google-token.json`:

1. **Migration script** (`scripts/migrate-tokens.py`):
   - Reads existing `google-token.json` (google-auth SDK format)
   - **Schema translation** (google-auth SDK → Tauri TokenPayload):
     - `token` → `access_token`
     - `refresh_token` → `refresh_token` (same key)
     - `expiry` (ISO-8601 string) → `expires_at` (parse to Unix epoch integer)
     - `client_id` → `client_id` (same key)
     - `client_secret` → `client_secret` (same key)
     - `token_uri`, `scopes`, `universe_domain`, `account` → dropped (not needed)
   - Prompts for engagement ID (default: value from current `heartbeat.toml`)
   - Generates encryption key if not in `secrets.env`
   - Encrypts the translated payload with AES-256-GCM
   - Uploads to `engagements/{eid}/google_tokens/google` via Admin SDK
   - Backs up `google-token.json` to `google-token.json.bak`

2. **heartbeat.toml migration:**
   - `config.py` accepts both old flat format and new `[[engagements]]` array
   - If flat `engagement_id` is present and `[[engagements]]` is absent,
     auto-wraps into a single-element array (logged as deprecation warning)
   - Old `google_token_path` config is ignored when Firestore tokens are available

3. **Rollback:** If the new code fails, the operator can revert to the
   Phase E branch. The `google-token.json.bak` file restores the old path.

## Implementation sub-phases

| # | Scope | Commit |
|---|---|---|
| F.1 | This spec document | `docs: add Phase F token-sync spec` |
| F.2 | TS frontend: encrypted token write on OAuth success (`src/lib/firestore-tokens.ts`). Uses WebCrypto (browser API available in Tauri's webview) for AES-GCM + Firebase JS SDK for Firestore write. Also fixes refresh-token rotation bug in `token_refresh.rs`. | `feat(tauri): write encrypted OAuth tokens to Firestore` |
| F.3 | Python-side: encrypted token read in heartbeat, replace `google_auth.py`. Add `cryptography>=42.0,<44.0` as explicit dependency in `pyproject.toml` (currently only transitively available via `google-auth`). Uses `cryptography.hazmat.primitives.ciphers.aead.AESGCM` (NOT the lower-level `Cipher` API — AESGCM handles tag concatenation automatically). | `feat(heartbeat): read encrypted tokens from Firestore` |
| F.4 | Multi-engagement: `[[engagements]]` in config + tick iteration. **Error isolation:** each engagement runs in its own try/except. One broken engagement (bad token, decrypt failure, key version mismatch) MUST NOT block other engagements. Per-engagement error codes in telemetry. | `feat(heartbeat): multi-engagement tick orchestration` |
| F.5 | Migration script: `google-token.json` → Firestore | `feat(heartbeat): token migration script` |
| F.6 | Update install.sh, deploy-to-vm.sh, README | `chore(heartbeat): update install/deploy for token sync` |
| F.7 | Post-code adversarial challenge | (no commit — applies fixes) |
| F.8 | Deploy to elara-vm, verify BLR Gmail reads correctly | `chore: deploy Phase F to elara-vm` |

## Security considerations

- **Encrypted at rest in Firestore:** AES-256-GCM. Firestore's own encryption-at-rest
  (Google-managed) provides defense-in-depth.
- **Key never transits the network:** The encryption key is pre-shared out-of-band
  (operator copies it during install). Ciphertext transits; key does not.
- **Scopes are read-only:** `calendar.readonly` + `gmail.readonly`. Even if tokens
  are compromised, the attacker cannot modify calendar events or send emails.
- **Token revocation:** If a token is suspected compromised, the consultant revokes
  it in Google Account settings. Next heartbeat tick gets `oauth_refresh_failed`,
  logs the error, and the operator re-authenticates via the Tauri app.
- **Admin SDK blast radius:** Same as Phase E — the service account has project-wide
  Firestore access. Accepted for single-tenant; deferred to Phase F+ for isolation.

## Pre-code challenge results

Challenge agent found 3 showstoppers, 5 blocks, 4 warns. All showstoppers
and blocks addressed in this revision:

| # | Severity | Issue | Resolution |
|---|---|---|---|
| 1 | SHOWSTOPPER | Refresh-token rotation race; spec assumed stable refresh_token | Added "Refresh-token rotation handling" section. Both sides must capture new refresh_token. Optimistic-concurrency check on heartbeat writeback. Also fixing existing Tauri bug in F.2. |
| 2 | SHOWSTOPPER | GCM auth tag not specified; cross-platform interop risk | Document schema now explicitly states `ciphertext = ciphertext \|\| 16-byte GCM tag`. Cross-platform test vector added. |
| 3 | SHOWSTOPPER | `cryptography` package missing from heartbeat deps | F.3 scope now includes adding explicit dependency. |
| 4 | BLOCK | WebCrypto doesn't exist in Rust; unclear which runtime | F.2 scope clarifies: TS frontend module using WebCrypto + Firebase JS SDK. |
| 5 | BLOCK | consultantId may be stale, blocking client SDK writes | Pre-deployment requirement added to Firestore rules section. Verify/fix before coding. |
| 6 | BLOCK | Key version fallback underspecified | New "Key rotation config" section with explicit env var shape, read/write logic, error codes. |
| 7 | BLOCK | Migration schema mismatch (google-auth SDK vs TokenPayload) | Field mapping table added to migration script scope. |
| 8 | BLOCK | Subcollection rules must be nested, not top-level | Rules section now specifies nesting inside existing engagements match block. |
| 9 | WARN | No error isolation between engagements | F.4 scope now mandates per-engagement try/except. |
| 10 | WARN | Base64 key encoding ambiguity | Key distribution section now explicitly states base64-decode requirement. |
| 11 | WARN | No key recovery path | Recovery procedure documented in key distribution section. |
| 12 | WARN | updatedAt should use server timestamps | Document schema changed from ISO-8601 string to `FieldValue.serverTimestamp()`. |
