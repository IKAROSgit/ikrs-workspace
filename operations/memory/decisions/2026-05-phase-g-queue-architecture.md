---
last_updated: 2026-05-06
updated_by: mac-claude (H.1 seeding)
sources: [docs/specs/m3-phase-g-proactive-intelligence.md]
---

# Decision: Queue-based single-writer for bot poller

- **Date:** 2026-05-04
- **Context:** Phase G adds a Telegram bot poller (long-running process)
  alongside the hourly tick (oneshot). Both need to interact with Firestore
  and local files. The adversarial challenge found this was a showstopper
  race condition.
- **Decision:** Queue-based single-writer. Poller writes ONLY to a Firestore
  command queue. Tick is the sole writer to ikrs_tasks, local files, and
  observations. No shared mutable state between processes.
- **Rationale:** Eliminates all inter-process coordination without locks,
  transactions, or shared state machines. The tick processes the queue at
  the start of each run.
