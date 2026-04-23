import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTaskStore } from "@/stores/taskStore";
import { useEngagementStore } from "@/stores/engagementStore";
import { useEngagementActions } from "@/providers/EngagementProvider";
import { markTaskPendingLocal } from "@/hooks/useTaskVaultBridge";
import type { Task, TaskStatus, TaskPriority } from "@/types";

/** Monotonic sort-order generator shared with useTaskVaultBridge
 *  via a separate instance each. `Date.now() * 1000 + counter`
 *  avoids collisions on rapid rapid-fire adds / paste / Claude
 *  auto-creates within a single millisecond. */
let _sortCounter = 0;
function nextSortOrder(): number {
  _sortCounter = (_sortCounter + 1) % 1000;
  return Date.now() * 1000 + _sortCounter;
}

/** Per-task serialization queue for vault mirror writes.
 *
 *  Codex 2026-04-21 pre-push flagged a real race: two quick edits
 *  on the same task (e.g. rename then status change) each fire an
 *  independent `write_task_frontmatter`. Because each read-modifies-
 *  then-writes the YAML, the later one may read before the earlier
 *  one's write lands and clobber the intermediate state.
 *
 *  Fix: chain per-task writes through a Promise keyed by taskId.
 *  When the chain becomes idle (completes without new enqueues) we
 *  drop the map entry to avoid leaking memory for long-lived
 *  boards. */
const _vaultQueues = new Map<string, Promise<void>>();
function enqueueVaultMirror(taskId: string, op: () => Promise<void>): Promise<void> {
  const prev = _vaultQueues.get(taskId) ?? Promise.resolve();
  // Chain regardless of prev outcome — a failed prior write must
  // not poison later ones (each op is idempotent to the file's
  // eventual state).
  const next = prev.catch(() => undefined).then(op);
  _vaultQueues.set(taskId, next);
  // Clean up when the chain drains, but only if this `next` is
  // still the tail — otherwise a later enqueue already took over.
  void next.finally(() => {
    if (_vaultQueues.get(taskId) === next) {
      _vaultQueues.delete(taskId);
    }
  });
  return next;
}

/**
 * Tasks hook — single surface over the zustand store + Firestore-
 * backed `EngagementProvider` actions.
 *
 * Exposes both legacy operations (`addTask`, `remove`) and the
 * 2026-04-21 Kanban MVP additions (`changeStatus`, `setPriority`,
 * `reorderWithinColumn`, `addNote`, `setClientVisible`).
 *
 * All mutations resolve the engagement's defaultClientVisibility
 * lazily so a new card picks up the engagement's posture at
 * creation time. Subsequent edits to engagement posture DO NOT
 * retroactively change existing cards — that's by design (avoids
 * unexpected client-visibility flips).
 */
export function useTasks() {
  const tasks = useTaskStore((s) => s.tasks);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagements = useEngagementStore((s) => s.engagements);
  const {
    createTask,
    updateTask,
    deleteTask,
    addTaskNote,
    setTaskClientVisible,
    changeTaskStatus,
  } = useEngagementActions();

  /** Resolve the default clientVisible for a new card in the active
   *  engagement. `undefined` engagement setting → open-book = true. */
  const resolveDefaultVisible = useCallback((): boolean => {
    const eng = engagements.find((e) => e.id === activeEngagementId);
    const setting = eng?.settings?.defaultClientVisibility ?? "open-book";
    return setting === "open-book";
  }, [activeEngagementId, engagements]);

  /** Resolve the client-slug (vault folder name) for the active
   *  engagement — needed for the vault-write path. */
  const clients = useEngagementStore((s) => s.clients);
  const resolveClientSlug = useCallback((): string | null => {
    const eng = engagements.find((e) => e.id === activeEngagementId);
    if (!eng) return null;
    return clients.find((c) => c.id === eng.clientId)?.slug ?? null;
  }, [activeEngagementId, engagements, clients]);

  // resolveVaultPath removed 2026-04-23 — reverted to passing null
  // for `vault_path` in Rust commands, letting them default to the
  // slug-derived path. Custom vault paths are deferred until we
  // actually onboard an engagement that uses one.

  /** Fire-and-forget vault-mirror of a task edit. Safe to call from
   *  any UI action — if the vault file doesn't exist yet this
   *  creates it; if it does, existing note body is preserved
   *  verbatim. `markTaskPendingLocal` is called FIRST so the
   *  watcher's echo event is suppressed by the 2s anti-flicker
   *  window. Failures are logged but not surfaced to the UI — the
   *  Firestore write is authoritative for the Kanban. */
  const mirrorToVault = useCallback(
    (
      taskId: string,
      patch: {
        title?: string;
        status?: TaskStatus;
        priority?: TaskPriority;
        tags?: string[];
        due?: string | null;
        client_visible?: boolean | null;
        description?: string;
        assignee?: string;
      },
    ): Promise<void> => {
      const slug = resolveClientSlug();
      if (!slug) return Promise.resolve();
      // Mark pending OUTSIDE the queue so the 2s anti-flicker window
      // starts at mutation time (not at eventual write time). This
      // matters when multiple writes are queued — all of their
      // vault-echo events need to be suppressed from t=0.
      markTaskPendingLocal(taskId);
      // Per-task serialization — writes on the same taskId run in
      // the order they were enqueued, so a fast title+status pair
      // can't interleave and lose one field.
      //
      // 2026-04-23: not passing vaultPath so Rust uses slug-derived
      // default, matching start_task_watch. Deferred until a future
      // engagement actually uses a non-default vault path.
      return enqueueVaultMirror(taskId, async () => {
        try {
          await invoke("write_task_frontmatter", {
            clientSlug: slug,
            vaultPath: null,
            patch: { id: taskId, ...patch },
          });
        } catch (e) {
          // eslint-disable-next-line no-console
          console.warn("[mirrorToVault] failed", e);
        }
      });
    },
    [resolveClientSlug],
  );

  const addTask = useCallback(
    async (title: string, priority: TaskPriority = "p2") => {
      if (!activeEngagementId) return;
      const clientVisible = resolveDefaultVisible();
      // Let Firestore assign the id on create, then mirror to vault
      // with that id so the two stay aligned from t=0.
      const newId = await createTask({
        engagementId: activeEngagementId,
        title,
        status: "backlog",
        priority,
        tags: [],
        subtasks: [],
        sortOrder: nextSortOrder(),
        source: "manual",
        clientVisible,
        assignee: "consultant",
        notesCount: 0,
        driveLinks: [],
      });
      // Fire-and-forget vault mirror. No await — don't block the UI.
      void mirrorToVault(newId, {
        title,
        status: "backlog",
        priority,
        client_visible: clientVisible,
      });
    },
    [activeEngagementId, createTask, resolveDefaultVisible, mirrorToVault],
  );

  const changeStatus = useCallback(
    async (taskId: string, nextStatus: TaskStatus) => {
      await changeTaskStatus(taskId, nextStatus);
      void mirrorToVault(taskId, { status: nextStatus });
    },
    [changeTaskStatus, mirrorToVault],
  );

  const setPriority = useCallback(
    async (taskId: string, priority: TaskPriority) => {
      await updateTask(taskId, { priority });
      void mirrorToVault(taskId, { priority });
    },
    [updateTask, mirrorToVault],
  );

  const reorderWithinColumn = useCallback(
    async (taskId: string, newSortOrder: number) => {
      // sortOrder is UI-only — don't mirror to vault (frontmatter
      // doesn't carry ordering; it'd churn the file for no value).
      await updateTask(taskId, { sortOrder: newSortOrder });
    },
    [updateTask],
  );

  const remove = useCallback(
    async (taskId: string) => {
      // We don't delete the vault file — Moe may still want the
      // history. Firestore deletion is authoritative for the board;
      // the vault file becomes orphaned until a manual cleanup.
      await deleteTask(taskId);
    },
    [deleteTask],
  );

  const addDriveLink = useCallback(
    async (taskId: string, url: string) => {
      const task = tasks.find((t) => t.id === taskId);
      if (!task) return;
      const links = Array.from(new Set([...(task.driveLinks ?? []), url]));
      await updateTask(taskId, { driveLinks: links });
    },
    [tasks, updateTask],
  );

  const removeDriveLink = useCallback(
    async (taskId: string, url: string) => {
      const task = tasks.find((t) => t.id === taskId);
      if (!task) return;
      const links = (task.driveLinks ?? []).filter((u) => u !== url);
      await updateTask(taskId, { driveLinks: links });
    },
    [tasks, updateTask],
  );

  const setTitle = useCallback(
    async (taskId: string, title: string) => {
      await updateTask(taskId, { title });
      void mirrorToVault(taskId, { title });
    },
    [updateTask, mirrorToVault],
  );

  const setDescription = useCallback(
    async (taskId: string, description: string) => {
      await updateTask(taskId, { description });
      void mirrorToVault(taskId, { description });
    },
    [updateTask, mirrorToVault],
  );

  const setDueDate = useCallback(
    async (taskId: string, dueDate: Date | undefined) => {
      await updateTask(taskId, { dueDate });
      void mirrorToVault(taskId, {
        due: dueDate ? dueDate.toISOString().slice(0, 10) : null,
      });
    },
    [updateTask, mirrorToVault],
  );

  const setClientVisible = useCallback(
    async (taskId: string, visible: boolean) => {
      await setTaskClientVisible(taskId, visible);
      void mirrorToVault(taskId, { client_visible: visible });
    },
    [setTaskClientVisible, mirrorToVault],
  );

  return {
    tasks,
    addTask,
    changeStatus,
    setPriority,
    reorderWithinColumn,
    remove,
    addNote: addTaskNote,
    setClientVisible,
    addDriveLink,
    removeDriveLink,
    setTitle,
    setDescription,
    setDueDate,
    // Back-compat: some old callers may import toggleStatus; keep a
    // thin shim that advances backlog → in_progress → done → backlog
    // so nothing breaks during the transition.
    toggleStatus: (task: Task) => {
      const cycle: Record<TaskStatus, TaskStatus> = {
        backlog: "in_progress",
        in_progress: "in_review",
        awaiting_client: "blocked",
        blocked: "in_progress",
        in_review: "done",
        done: "backlog",
      };
      return changeTaskStatus(task.id, cycle[task.status]);
    },
  };
}
