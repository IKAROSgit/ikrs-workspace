import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  collection,
  doc,
  getDocs,
  query,
  serverTimestamp,
  setDoc,
  updateDoc,
  where,
} from "firebase/firestore";
import { db } from "@/lib/firebase";
import { useEngagementStore } from "@/stores/engagementStore";
import { useTaskStore } from "@/stores/taskStore";
import type { Task, TaskPriority, TaskStatus, TaskAssignee } from "@/types";

/**
 * Frontmatter payload the Rust task_watch watcher emits when a
 * markdown file under `02-tasks/` is written by Claude. Shape must
 * match `TaskFrontmatter` in `src-tauri/src/commands/task_watch.rs`.
 */
interface TaskVaultChangePayload {
  id: string;
  title: string;
  status: string;
  priority: string;
  tags: string[];
  due: string | null;
  client_visible: boolean | null;
  description: string | null;
  assignee: string;
  vault_path: string;
  engagement_id: string;
}

/** 2000ms window during which a locally-initiated drag or edit
 *  suppresses incoming vault-change upserts for the same task id.
 *  Prevents mid-animation UI snap-back per Codex §B.6.
 *
 *  Calibrated against: Rust-side notify-debounce (250ms) + worst-
 *  case Firestore write commit latency on a slow link (~1s p95) +
 *  headroom. The earlier 250ms matched the Rust debounce
 *  coincidentally but was way too tight — pre-ship Codex audit
 *  2026-04-21 flagged a late-echo window where a second notify
 *  event at T0+300ms would bypass the guard and clobber the
 *  drag. */
const LOCAL_PENDING_WINDOW_MS = 2000;

/** Per-tab per-active-engagement registry of "I just edited this
 *  card locally, hold off on applying vault echoes". Cleared after
 *  the window expires. */
const pendingLocalEdits = new Map<string, number>();

export function markTaskPendingLocal(taskId: string) {
  pendingLocalEdits.set(taskId, Date.now());
  // Opportunistic cleanup.
  setTimeout(() => {
    const t = pendingLocalEdits.get(taskId);
    if (t && Date.now() - t >= LOCAL_PENDING_WINDOW_MS) {
      pendingLocalEdits.delete(taskId);
    }
  }, LOCAL_PENDING_WINDOW_MS + 50);
}

function isWithinLocalWindow(taskId: string): boolean {
  const t = pendingLocalEdits.get(taskId);
  return !!t && Date.now() - t < LOCAL_PENDING_WINDOW_MS;
}

/**
 * Mounts the vault→Firestore task bridge for the active engagement.
 *
 * On mount:
 *   - Calls Rust `start_task_watch` for the active engagement/slug
 *   - Subscribes to `task:vault-change` Tauri events
 *   - For each event: normalises the frontmatter, applies the
 *     anti-flicker guard, then upserts the matching Firestore doc
 *     (keyed by the frontmatter `id`)
 *
 * On unmount / engagement switch: stops the watcher.
 *
 * The real-time Firestore listener in `EngagementProvider` then
 * fans the upsert out to the Kanban UI with no additional code.
 *
 * Mount this once in `TasksView` — cheap, idempotent across
 * re-renders thanks to ref guards.
 */
export function useTaskVaultBridge() {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  // Derive clientSlug + vaultPath via primitive string identity so
  // effect deps don't churn when unrelated engagement fields
  // (notesCount on a task, etc.) update the engagement list
  // reference.
  const clientSlug = useEngagementStore((s) => {
    const eng = s.engagements.find((e) => e.id === s.activeEngagementId);
    if (!eng) return null;
    return s.clients.find((c) => c.id === eng.clientId)?.slug ?? null;
  });
  // 2026-04-22 bug fix: pass the authoritative vault path (same one
  // Claude's CLI has as cwd) so the watcher looks where files are
  // actually written. Without this, engagements with Drive-synced
  // vault.path values silently dropped every Claude-authored task.
  const vaultPath = useEngagementStore((s) => {
    const eng = s.engagements.find((e) => e.id === s.activeEngagementId);
    return eng?.vault.path ?? null;
  });

  // Resolve engagement default-client-visibility lazily through a
  // getter so the effect doesn't re-fire on every engagement
  // settings edit. Codex 2026-04-21 pre-push must-fix: watcher
  // churn on every Firestore engagement write.
  const engagementSettingsRef = useRef<
    ReturnType<typeof useEngagementStore.getState>
  >(useEngagementStore.getState());
  useEffect(() => {
    const unsub = useEngagementStore.subscribe((s) => {
      engagementSettingsRef.current = s;
    });
    return unsub;
  }, []);

  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!activeEngagementId || !clientSlug) return;

    let cancelled = false;

    const setup = async () => {
      try {
        // 2026-04-23: pass `null` so Rust uses the slug-derived
        // default (`~/.ikrs-workspace/vaults/<slug>/02-tasks/`).
        // Symmetric with `write_task_frontmatter` which also
        // defaults to slug-derived when vaultPath is null.
        // Passing engagement.vault.path here was breaking sandbox
        // fs-capability scope and the skills validator. Deferred
        // until we actually onboard an engagement with a real
        // Drive-synced vault path.
        await invoke("start_task_watch", {
          engagementId: activeEngagementId,
          clientSlug,
          vaultPath: null,
        });
      } catch (e) {
        // eslint-disable-next-line no-console
        console.warn("[task-watch] start failed", e);
        return;
      }
      if (cancelled) return;

      const unlisten = await listen<TaskVaultChangePayload>(
        "task:vault-change",
        async (ev) => {
          const p = ev.payload;
          if (p.engagement_id !== activeEngagementId) return;
          if (isWithinLocalWindow(p.id)) {
            // eslint-disable-next-line no-console
            console.debug(
              "[task-watch] suppressing vault echo for locally-pending card",
              p.id,
            );
            return;
          }

          const status = normaliseStatus(p.status);
          const priority = normalisePriority(p.priority);
          const assignee = normaliseAssignee(p.assignee);
          const due = p.due ? safeDate(p.due) : undefined;

          // Decide create-vs-update by querying Firestore for the id.
          // Firestore auto-IDs if we addDoc — we set the doc id
          // explicitly to the frontmatter id so Claude's vault file
          // and the Firestore doc line up.
          const taskRef = doc(db, "ikrs_tasks", p.id);
          const existing = useTaskStore
            .getState()
            .tasks.find((t) => t.id === p.id);

          if (existing) {
            // Update path — don't stomp sortOrder or clientVisible
            // unless frontmatter explicitly declares a new value.
            // Narrow the patch type through the firestore Partial
            // interface instead of Record<string, unknown> so
            // Firestore's FieldValue union is honoured.
            const patch: Partial<Task> & {
              updatedAt?: ReturnType<typeof serverTimestamp>;
            } = {
              title: p.title,
              status,
              priority,
              assignee,
              description: p.description ?? "",
              tags: p.tags ?? [],
              vaultPath: p.vault_path,
              updatedAt: serverTimestamp() as unknown as undefined,
            };
            if (due !== undefined) patch.dueDate = due;
            if (p.client_visible !== null && p.client_visible !== undefined) {
              patch.clientVisible = p.client_visible;
              if (p.client_visible === true && !existing.sharedAt) {
                patch.sharedAt = new Date();
              }
            }
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            await updateDoc(taskRef, patch as any);
          } else {
            // Create path — use setDoc so the Firestore id matches
            // the frontmatter id.
            // Resolve default client-visibility from the latest
            // engagement snapshot through the ref (not captured
            // at hook-mount time).
            const store = engagementSettingsRef.current;
            const currentEng = store.engagements.find(
              (e) => e.id === activeEngagementId,
            );
            const defaultOpenBook =
              (currentEng?.settings?.defaultClientVisibility ??
                "open-book") === "open-book";
            const clientVisible = p.client_visible ?? defaultOpenBook;

            await setDoc(taskRef, {
              _v: 2,
              id: p.id,
              engagementId: activeEngagementId,
              title: p.title,
              description: p.description ?? "",
              status,
              priority,
              tags: p.tags ?? [],
              subtasks: [],
              sortOrder: nextSortOrder(),
              source: "claude",
              clientVisible,
              assignee,
              vaultPath: p.vault_path,
              driveLinks: [],
              notesCount: 0,
              ...(due !== undefined ? { dueDate: due } : {}),
              createdAt: serverTimestamp(),
              updatedAt: serverTimestamp(),
            });
          }
        },
      );

      if (cancelled) {
        unlisten();
        return;
      }
      unlistenRef.current = unlisten;

      // 2026-04-29: trigger the initial scan AFTER the listener is
      // attached. Previously the scan ran inside start_task_watch
      // (on a std::thread), racing the listener — 5 of 8 events
      // were lost because they fired before listen() returned.
      try {
        await invoke("trigger_task_scan");
      } catch (e) {
        // Non-fatal: watcher still fires on new filesystem events.
        // eslint-disable-next-line no-console
        console.warn("[task-watch] initial scan trigger failed", e);
      }
    };

    void setup();

    return () => {
      cancelled = true;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
      // Fire and forget — if this fails (e.g. engagement already
      // swapped out) the watcher will be replaced by the next
      // start_task_watch anyway.
      void invoke("stop_task_watch").catch(() => {});
    };
    // Only two stable-identity primitives: the active engagement id
    // and its derived client-slug. Do NOT add the engagement object
    // here; that reference changes on every Firestore write to the
    // engagements collection and would restart the notify watcher
    // (dropping in-flight events). Codex 2026-04-21 pre-push fix.
  }, [activeEngagementId, clientSlug, vaultPath]);
}

/** Monotonic sort-order generator. `Date.now()` alone collides when
 *  two task creates land within a single millisecond (fast paste,
 *  claude-auto-create-then-drag). This stacks a per-instance
 *  counter so values are always strictly increasing. */
let _sortCounter = 0;
function nextSortOrder(): number {
  _sortCounter = (_sortCounter + 1) % 1000;
  return Date.now() * 1000 + _sortCounter;
}

function normaliseStatus(s: string): TaskStatus {
  switch (s) {
    case "todo":
      return "backlog"; // legacy alias
    case "backlog":
    case "in_progress":
    case "awaiting_client":
    case "blocked":
    case "in_review":
    case "done":
      return s;
    default:
      return "backlog";
  }
}

function normalisePriority(p: string): TaskPriority {
  switch (p) {
    case "p1":
    case "p2":
    case "p3":
      return p;
    default:
      return "p2";
  }
}

function normaliseAssignee(a: string): TaskAssignee {
  switch (a) {
    case "consultant":
    case "claude":
    case "client":
      return a;
    default:
      return "claude"; // files authored via vault watcher default to Claude
  }
}

function safeDate(iso: string): Date | undefined {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? undefined : d;
}

/**
 * Utility: preload existing tasks from Firestore and match them
 * against the local vault's 02-tasks/*.md files so a cold-start
 * TasksView doesn't miss any Claude-authored cards. Not needed for
 * MVP — the real-time listener in EngagementProvider already does
 * this implicitly — but retained here as a future-use export for
 * the "recompute" button on the TasksView header (day 2).
 */
export async function reconcileVaultAndFirestore(
  engagementId: string,
): Promise<number> {
  const q = query(
    collection(db, "ikrs_tasks"),
    where("engagementId", "==", engagementId),
  );
  const snap = await getDocs(q);
  // Currently no-op; placeholder for future vault/Firestore diff logic.
  return snap.size;
}

