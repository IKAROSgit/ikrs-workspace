# Issue C: Orphan Task Import (2026-05-04)

## Summary

47 markdown task files were stranded in `bar-world-com/02-tasks/` after
the three engagements pointing at that vault were deleted from Firestore.
The slug "bar-world-com" was a typo of "blr-world-com" created during
initial engagement setup.

## Operation

- **Timestamp:** 2026-05-04T16:47:43Z
- **Operator:** Claude Code (automated, authorized by Moe)
- **Script:** `heartbeat/scripts/import-orphan-vault-tasks.py`
- **Source vault:** `/Users/bigmac/.ikrs-workspace/vaults/bar-world-com`
- **Dest vault:** `/Users/bigmac/.ikrs-workspace/vaults/blr-world-com`
- **Files copied:** 47
- **Files skipped:** 0

## Sample task slugs

- `t-admin-ajc-followup` (AJC — follow up)
- `t-p1-001-upload-performance-hub` (Upload Performance Hub to Google Sites)
- `t-p3-retainer-extension-proposal` (Retainer Extension Proposal Apr-May)

## Post-import verification

- Total files in `blr-world-com/02-tasks/`: 56 (47 imported + 9 existing)
- On next app launch, `trigger_task_scan` syncs all 56 to Firestore
- Tasks appear in BLR engagement Kanban under their original statuses

## Root cause

Engagement deletion in Firebase Console does not cascade to:
- Vault folders on the Mac filesystem
- `ikrs_tasks` documents in Firestore

This is documented as a known hazard in `docs/ECOSYSTEM.md` §7 and
being addressed by Task 2 (orphan detection + recovery UI).
