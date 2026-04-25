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

## Quickstart (operator install)

```bash
# 1. Clone the workspace repo onto the VM (only the heartbeat/ folder is
#    actually used — the rest is harmless).
git clone https://github.com/ikaros-intelligence/ikrs-workspace.git
cd ikrs-workspace/heartbeat

# 2. Run the installer. It will:
#    - install python3.11 + venv + systemd units
#    - prompt once for Gemini API key + Firebase service-account JSON
#    - run the Google OAuth flow for Gmail + Calendar
#    - prompt for the Telegram bot token + chat ID
#    - enable the systemd timer and run a smoke tick
sudo ./scripts/install.sh

# 3. Verify
systemctl --user status ikrs-heartbeat.timer
journalctl --user -u ikrs-heartbeat -f
```

## Layout

```
heartbeat/
├── pyproject.toml
├── src/heartbeat/
│   ├── main.py            # entry point; --dry-run, --once, --config
│   ├── tick.py            # one tick: signals → llm → outputs → telemetry
│   ├── config.py          # loads heartbeat.toml + secrets.env
│   ├── telemetry.py       # heartbeat_health Firestore writer
│   ├── llm/gemini.py      # Gemini adapter (E.2)
│   ├── signals/{calendar,gmail,vault}.py  (E.3)
│   ├── outputs/{firestore,telegram,audit}.py  (E.5)
│   └── prompts/tick_prompt.v1.txt  (E.4)
├── tests/                 # pytest suite (run via `pytest` or CI)
├── systemd/
│   ├── ikrs-heartbeat.service
│   └── ikrs-heartbeat.timer
├── scripts/
│   ├── install.sh         # idempotent installer (E.6)
│   ├── uninstall.sh       # removes timer + service, preserves vault
│   └── smoke-test.sh      # forces one tick + asserts Firestore write
└── config/
    └── heartbeat.toml.example
```

## Configuration

`heartbeat.toml` (lives at `/etc/ikrs-heartbeat/heartbeat.toml` after install).
Non-secret config: tenant ID, engagement ID, vault path, prompt version.
Secrets live in `/etc/ikrs-heartbeat/secrets.env` (mode 0600, root-owned).

See `config/heartbeat.toml.example` for the schema.

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
- Firebase Admin SDK service account scoped to a single project; we accept
  project-wide blast radius for now (see Phase F for tenant isolation).
- Telegram bot is per-operator (BotFather, ~90 sec). No shared bot, no
  centrally-rotatable token.
- All secrets live in `/etc/ikrs-heartbeat/secrets.env` with `0600 root:root`.

## Out of scope (deferred)

- Multi-tenant Firebase project per consultant (Phase F)
- Cloud Function proxy for Firestore writes (Phase F)
- Bi-directional Telegram (Phase F)
- claude-cli on tenant VM (never — ToS)
- Anthropic API key path (until first commercial tenant brings one)
