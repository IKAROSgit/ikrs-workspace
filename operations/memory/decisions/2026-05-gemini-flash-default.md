---
last_updated: 2026-05-06
updated_by: mac-claude (H.1 seeding)
sources: [heartbeat/config/heartbeat.toml.example]
---

# Decision: Default heartbeat model to Gemini 2.5 Flash (not Pro)

- **Date:** 2026-05-04
- **Context:** Heartbeat runs ~720 ticks/month at ~3000 tokens each.
  Pro is 8-15x more expensive per token than Flash.
- **Decision:** Default to gemini-2.5-flash in heartbeat.toml.example.
  Operator can override to Pro if they measurably need Pro-tier reasoning.
- **Rationale:** Cost discipline. The hourly triage workload (unread email
  scan, calendar check, vault diff) doesn't require Pro-tier reasoning.
  Flash is sufficient for structured-output action emission.
