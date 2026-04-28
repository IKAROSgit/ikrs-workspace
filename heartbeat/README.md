# IKAROS Heartbeat (Tier II — Gemini on-VM)

Autonomous hourly companion that runs on a tiny Linux VM beside the operator's
Mac. Reads Gmail/Calendar/vault state, prompts Gemini for triage and analytics,
writes structured updates to Firestore, and pushes urgency-flagged items to
Telegram.

This is **Tier II** of the dual-tier heartbeat (see
`docs/specs/m3-phase-e-autonomous-heartbeat.md`). Tier I (Claude in-app while
the user is present) lives in `src-tauri/src/heartbeat/`. Both write to the
same Firestore collections and audit log.

## Why a separate VM (and not the Mac)

- Mac is closed at night, on planes, during travel — Tier I misses those hours.
- Anthropic Consumer Terms forbid unattended/automated Claude use. Gemini's
  paid tier explicitly permits commercial/non-human use, so Tier II uses
  Gemini and Tier I uses Claude.
- VM is small (1 vCPU / 1 GiB is plenty), cheap (~$5/mo), and templated for
  commercial replication.

## How OAuth tokens flow (Phase F)

1. **Mac (Tauri app):** Operator connects Google for each engagement in
   Settings. Tokens are stored in the Mac keychain AND encrypted
   (AES-256-GCM) + written to Firestore at
   `engagements/{eid}/google_tokens/google`.

2. **VM (heartbeat):** On each tick, the heartbeat reads the encrypted
   token from Firestore, decrypts with the operator's encryption key,
   and uses it for Gmail/Calendar API calls. If the access token is
   expired, it refreshes via Google's token endpoint and writes the
   updated encrypted token back to Firestore.

3. **Multi-engagement:** Repeat the OAuth connection in Tauri for each
   engagement. The heartbeat iterates `[[engagements]]` in
   `heartbeat.toml` and processes each one independently with error
   isolation.

4. **Legacy fallback:** If no Firestore token exists for an engagement
   (e.g., not yet migrated), the heartbeat falls back to the legacy
   `/etc/ikrs-heartbeat/google-token.json` file from Phase E.

## Quickstart (fresh install)

```bash
# 1. Clone the workspace repo onto the VM.
git clone https://github.com/IKAROSgit/ikrs-workspace.git
cd ikrs-workspace/heartbeat

# 2. Run the installer. It will:
#    - install python3.11 + venv + systemd units
#    - prompt for Gemini API key + Firebase service-account JSON
#    - generate the token encryption key (BACK IT UP!)
#    - prompt for Telegram bot token + chat ID
#    - enable the systemd timer and run a smoke tick
sudo ./scripts/install.sh

# 3. On the Mac, open IKAROS Workspace app → Settings → Connect Google
#    for each engagement. Tokens auto-sync to Firestore.

# 4. Copy the encryption key to the Mac's .env.local:
#    VITE_TOKEN_ENCRYPTION_KEY=<the key printed during install>

# 5. Verify
sudo systemctl status ikrs-heartbeat.timer
sudo journalctl -u ikrs-heartbeat -f
```

## Upgrading from Phase E (existing deployment)

```bash
# 1. Pull latest code + reinstall
cd ~/projects/apps/ikrs-workspace
git pull origin main
sudo /opt/ikrs-heartbeat/venv/bin/pip install -e heartbeat --quiet

# 2. Re-run install.sh (generates encryption key if missing)
sudo bash heartbeat/scripts/install.sh

# 3. Migrate the legacy token to Firestore
source /etc/ikrs-heartbeat/secrets.env
export TOKEN_ENCRYPTION_KEY TOKEN_ENCRYPTION_KEY_VERSION
sudo -E /opt/ikrs-heartbeat/venv/bin/python \
  heartbeat/scripts/migrate-token-to-firestore.py <engagement-id>

# 4. Verify next tick reads from Firestore
sudo systemctl start ikrs-heartbeat.service
sudo journalctl -u ikrs-heartbeat -n 50 --no-pager | grep -i firestore

# 5. After a successful tick, remove the legacy file
sudo rm /etc/ikrs-heartbeat/google-token.json
```

## Layout

```
heartbeat/
+-- pyproject.toml
+-- src/heartbeat/
|   +-- main.py                    # entry point; --dry-run, --once, --config
|   +-- tick.py                    # one tick: signals -> llm -> outputs -> telemetry
|   +-- config.py                  # loads heartbeat.toml + secrets.env
|   +-- telemetry.py               # heartbeat_health Firestore writer
|   +-- llm/gemini.py              # Gemini adapter (E.2)
|   +-- signals/{calendar,gmail,vault}.py   (E.3)
|   +-- signals/firestore_tokens.py         (F.3 — encrypted token read)
|   +-- signals/google_auth.py              (E.3 — legacy file-based auth)
|   +-- outputs/{firestore,telegram,audit}.py  (E.5)
|   +-- prompts/tick_prompt.v1.txt  (E.4)
+-- tests/                         # pytest suite (run via pytest or CI)
+-- systemd/
|   +-- ikrs-heartbeat.service
|   +-- ikrs-heartbeat.timer
+-- scripts/
|   +-- install.sh                 # idempotent installer
|   +-- uninstall.sh               # removes timer + service, preserves vault
|   +-- smoke-test.sh              # forces one tick + asserts Firestore write
|   +-- deploy-to-vm.sh            # rsync + remote install
|   +-- migrate-token-to-firestore.py  # Phase E -> F migration (F.5)
+-- config/
    +-- heartbeat.toml.example
```

## Configuration

`heartbeat.toml` (lives at `/etc/ikrs-heartbeat/heartbeat.toml` after install).
Non-secret config: tenant ID, engagements array, vault paths, LLM knobs.
Secrets live in `/etc/ikrs-heartbeat/secrets.env` (mode 0600).

See `config/heartbeat.toml.example` for the schema. Two formats are supported:

- **Phase F (preferred):** `[[engagements]]` array with `id` + `vault_root`
  per engagement.
- **Legacy (Phase E):** Flat `engagement_id` + `vault_root` at top level.
  Auto-wrapped into a single-element array with a deprecation warning.

## Key rotation

If the encryption key is compromised or needs rotation:

1. Generate a new key: `openssl rand -base64 32`
2. On the VM, edit `/etc/ikrs-heartbeat/secrets.env`:
   - Move `TOKEN_ENCRYPTION_KEY` to `TOKEN_ENCRYPTION_KEY_PREV`
   - Move `TOKEN_ENCRYPTION_KEY_VERSION` to `TOKEN_ENCRYPTION_KEY_PREV_VERSION`
   - Set the new key as `TOKEN_ENCRYPTION_KEY`, bump version
3. Restart the heartbeat: `sudo systemctl restart ikrs-heartbeat.service`
4. The next tick reads with the old key, writes back with the new key
   (automatic re-encryption per engagement).
5. After one full cycle (all engagements ticked), remove the `_PREV` entries.
6. Update the Mac's `.env.local` with the new `VITE_TOKEN_ENCRYPTION_KEY`.

Note: key rotation does not require re-OAuthing. The tokens themselves are
unchanged; only the encryption wrapper is replaced.

## Running locally (no install, no VM)

```bash
cd heartbeat
python3.11 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"

# Dry-run: parse config, print what *would* happen, no LLM call, no writes.
python -m heartbeat.main --dry-run --config config/heartbeat.toml.example

# Tests
pytest
ruff check src tests
mypy src
```

## Tier I/II coordination

If Tier II wrote a `heartbeat_health` doc within the last 30 minutes, Tier I
no-ops the write side and only does verification on its next tick. Source of
truth: the `heartbeat_health` collection, indexed by `(tenantId, engagementId,
tickTs)`.

## Security posture

- No IKAROS-held API keys. Operator's own Gemini paid-tier key.
- OAuth tokens encrypted at rest in Firestore (AES-256-GCM) with operator key.
- Firebase Admin SDK service account scoped to a single project; we accept
  project-wide blast radius for now.
- Telegram bot is per-operator (BotFather, ~90 sec). No shared bot, no
  centrally-rotatable token.
- All secrets live in `/etc/ikrs-heartbeat/secrets.env` with `0600`.

## Out of scope (deferred)

- Multi-tenant Firebase project per consultant
- Cloud Function proxy for Firestore writes
- Bi-directional Telegram
- claude-cli on tenant VM (never -- ToS)
- Anthropic API key path (until first commercial tenant brings one)
- Automated key rotation script (manual procedure documented above)
