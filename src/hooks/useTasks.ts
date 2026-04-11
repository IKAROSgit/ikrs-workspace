import { useCallback } from "react";
import { useTaskStore } from "@/stores/taskStore";
import { useEngagementStore } from "@/stores/engagementStore";
import { useEngagementActions } from "@/providers/EngagementProvider";
import type { Task, TaskStatus, TaskPriority } from "@/types";

export function useTasks() {
  const tasks = useTaskStore((s) => s.tasks);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const { createTask, updateTask, deleteTask } = useEngagementActions();

  const addTask = useCallback(
    async (title: string, priority: TaskPriority = "p2") => {
      if (!activeEngagementId) return;
      await createTask({
        engagementId: activeEngagementId,
        title,
        status: "todo",
        priority,
        tags: [],
        subtasks: [],
        sortOrder: tasks.length,
        source: "manual",
      });
    },
    [activeEngagementId, createTask, tasks.length],
  );

  const toggleStatus = useCallback(
    async (task: Task) => {
      const nextStatus: Record<TaskStatus, TaskStatus> = {
        todo: "in_progress",
        in_progress: "done",
        done: "todo",
      };
      await updateTask(task.id, { status: nextStatus[task.status] });
    },
    [updateTask],
  );

  const remove = useCallback(
    async (taskId: string) => {
      await deleteTask(taskId);
    },
    [deleteTask],
  );

  const changePriority = useCallback(
    async (taskId: string, priority: TaskPriority) => {
      await updateTask(taskId, { priority });
    },
    [updateTask],
  );

  return { tasks, addTask, toggleStatus, remove, changePriority };
}
