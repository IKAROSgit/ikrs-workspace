---
last_updated: 2026-05-06
updated_by: mac-claude (H.1 seeding)
sources: [docs/specs/m3-phase-f-token-sync.md]
---

# Decision: AES-256-GCM with operator key (not Cloud KMS)

- **Date:** 2026-04-28
- **Context:** Phase F needed encryption for OAuth tokens synced to Firestore.
  Two options: operator-supplied AES key or Google Cloud KMS.
- **Decision:** Operator-supplied AES-256-GCM key.
- **Rationale:** KMS adds latency on every tick, a new GCP dependency, and
  setup friction — disproportionate for the threat model. The operator key
  sits at the same security level as the Firebase service account JSON.
- **Consequence:** Key must be backed up manually. Lost key = all tokens
  unreadable, every engagement must re-OAuth.
