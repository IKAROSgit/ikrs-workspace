import { useCallback } from "react";
import { useTaskStore } from "@/stores/taskStore";
import { useEngagementStore } from "@/stores/engagementStore";
import { useEngagementActions } from "@/providers/EngagementProvider";
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

  const addTask = useCallback(
    async (title: string, priority: TaskPriority = "p2") => {
      if (!activeEngagementId) return;
      await createTask({
        engagementId: activeEngagementId,
        title,
        status: "backlog",
        priority,
        tags: [],
        subtasks: [],
        sortOrder: nextSortOrder(),
        source: "manual",
        clientVisible: resolveDefaultVisible(),
        assignee: "consultant",
        notesCount: 0,
        driveLinks: [],
      });
    },
    [activeEngagementId, createTask, resolveDefaultVisible],
  );

  const changeStatus = useCallback(
    async (taskId: string, nextStatus: TaskStatus) => {
      await changeTaskStatus(taskId, nextStatus);
    },
    [changeTaskStatus],
  );

  const setPriority = useCallback(
    async (taskId: string, priority: TaskPriority) => {
      await updateTask(taskId, { priority });
    },
    [updateTask],
  );

  const reorderWithinColumn = useCallback(
    async (taskId: string, newSortOrder: number) => {
      await updateTask(taskId, { sortOrder: newSortOrder });
    },
    [updateTask],
  );

  const remove = useCallback(
    async (taskId: string) => {
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
    },
    [updateTask],
  );

  const setDescription = useCallback(
    async (taskId: string, description: string) => {
      await updateTask(taskId, { description });
    },
    [updateTask],
  );

  const setDueDate = useCallback(
    async (taskId: string, dueDate: Date | undefined) => {
      await updateTask(taskId, { dueDate });
    },
    [updateTask],
  );

  return {
    tasks,
    addTask,
    changeStatus,
    setPriority,
    reorderWithinColumn,
    remove,
    addNote: addTaskNote,
    setClientVisible: setTaskClientVisible,
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
