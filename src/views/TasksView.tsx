import { useMemo, useState, useCallback, lazy, Suspense } from "react";
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  closestCorners,
  DragOverlay,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { arrayMove } from "@dnd-kit/sortable";
import { Plus, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useTasks } from "@/hooks/useTasks";
import {
  useTaskVaultBridge,
  markTaskPendingLocal,
} from "@/hooks/useTaskVaultBridge";
import { useEngagementStore } from "@/stores/engagementStore";
import { useEngagementActions } from "@/providers/EngagementProvider";
import { KanbanColumn, KANBAN_COLUMNS } from "@/components/tasks/KanbanColumn";
import { TaskCard } from "@/components/tasks/TaskCard";
import type { Task, TaskStatus, ClientVisibilityDefault } from "@/types";

// Drawer is lazy-loaded — not needed until a card is opened; also
// avoids pulling react-markdown into the initial TasksView chunk.
const TaskDetailDrawer = lazy(() =>
  import("@/components/tasks/TaskDetailDrawer").then((m) => ({
    default: m.TaskDetailDrawer,
  })),
);

/**
 * Kanban board. Six columns (see KANBAN_COLUMNS), drag-and-drop
 * across and within. Clicking a card opens the detail drawer on
 * the right.
 *
 * Engagement-wide defaultClientVisibility is a small inline
 * toggle at the top-right of the board — lets Moe flip an
 * engagement between "open-book" (new cards default visible to
 * client) and "private" (new cards default hidden).
 *
 * Drag constraint: PointerSensor with `distance: 8` so clicks on
 * cards open the drawer rather than being treated as zero-distance
 * drags. Codex §B.3 density target: 6 columns on 1280px → ~195px
 * each, so horizontal scrolling below ~1200px viewport via a wide
 * flex container with overflow-x-auto.
 */
export default function TasksView() {
  const { tasks, addTask, changeStatus, reorderWithinColumn } = useTasks();
  // Bridge Claude's vault writes into Firestore Kanban state.
  useTaskVaultBridge();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId),
  );
  const { updateEngagement } = useEngagementActions();

  const [newTitle, setNewTitle] = useState("");
  const [openId, setOpenId] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    }),
  );

  const columns = useMemo(() => {
    const buckets: Record<TaskStatus, Task[]> = {
      backlog: [],
      in_progress: [],
      awaiting_client: [],
      blocked: [],
      in_review: [],
      done: [],
    };
    for (const t of tasks) {
      (buckets[t.status] ?? buckets.backlog).push(t);
    }
    for (const k of Object.keys(buckets) as TaskStatus[]) {
      buckets[k].sort((a, b) => a.sortOrder - b.sortOrder);
    }
    return buckets;
  }, [tasks]);

  const openTask = useMemo(
    () => (openId ? tasks.find((t) => t.id === openId) ?? null : null),
    [openId, tasks],
  );

  const activeTask = useMemo(
    () => (activeId ? tasks.find((t) => t.id === activeId) ?? null : null),
    [activeId, tasks],
  );

  const onDragStart = useCallback((e: DragStartEvent) => {
    setActiveId(String(e.active.id));
  }, []);

  const onDragEnd = useCallback(
    async (e: DragEndEvent) => {
      setActiveId(null);
      const { active, over } = e;
      if (!over) return;
      const activeTask = tasks.find((t) => t.id === active.id);
      if (!activeTask) return;

      // Dropped on a column container (empty column or column body
      // outside any card) → just change status. Mark pending-local
      // so the vault watcher can't echo-clobber this write during
      // the 250ms anti-flicker window.
      if (
        typeof over.id === "string" &&
        over.id.startsWith("column-")
      ) {
        const nextStatus = over.id.slice("column-".length) as TaskStatus;
        if (nextStatus !== activeTask.status) {
          markTaskPendingLocal(activeTask.id);
          await changeStatus(activeTask.id, nextStatus);
        }
        return;
      }

      // Dropped on another card.
      const overTask = tasks.find((t) => t.id === over.id);
      if (!overTask) return;

      if (activeTask.status !== overTask.status) {
        // Cross-column drop: update status. sortOrder is handled by
        // the receiving column's natural ordering (we'd need a
        // multi-write for precise position — defer per Codex §B
        // MVP).
        markTaskPendingLocal(activeTask.id);
        await changeStatus(activeTask.id, overTask.status);
        return;
      }

      // Same-column reorder — recompute sortOrder using arrayMove.
      const col = columns[activeTask.status];
      const oldIndex = col.findIndex((t) => t.id === activeTask.id);
      const newIndex = col.findIndex((t) => t.id === overTask.id);
      if (oldIndex === -1 || newIndex === -1 || oldIndex === newIndex) {
        return;
      }
      const reordered = arrayMove(col, oldIndex, newIndex);
      // Assign monotonically increasing sortOrder values so the
      // set reflects the new arrangement. Each gets a distinct
      // timestamp — future inserts land at .now(), which will be
      // higher than any existing.
      // Monotonically-increasing sort values that won't collide with
      // the one-millisecond-resolution Date.now() used elsewhere:
      // multiply by 1000 + index gives room for any same-ms drags.
      const base = Date.now() * 1000;
      for (let i = 0; i < reordered.length; i++) {
        const t = reordered[i]!;
        markTaskPendingLocal(t.id);
        void reorderWithinColumn(t.id, base + i);
      }
    },
    [tasks, columns, changeStatus, reorderWithinColumn],
  );

  const handleAdd = async () => {
    const title = newTitle.trim();
    if (!title) return;
    await addTask(title);
    setNewTitle("");
  };

  const cycleDefaultVisibility = async () => {
    if (!engagement) return;
    const cur =
      engagement.settings.defaultClientVisibility ?? "open-book";
    const next: ClientVisibilityDefault =
      cur === "open-book" ? "private" : "open-book";
    await updateEngagement(engagement.id, {
      settings: { ...engagement.settings, defaultClientVisibility: next },
    });
  };

  if (!activeEngagementId) {
    return (
      <div className="flex flex-col h-full p-4">
        <h2 className="text-lg font-semibold mb-4">Tasks</h2>
        <p className="text-muted-foreground">
          Select an engagement to manage tasks.
        </p>
      </div>
    );
  }

  const defaultVisibility =
    engagement?.settings?.defaultClientVisibility ?? "open-book";

  return (
    <div className="flex flex-col h-full">
      {/* Header: add + engagement visibility default */}
      <div className="flex items-center gap-2 px-4 py-2 border-b border-border">
        <Input
          placeholder="New task — press Enter"
          value={newTitle}
          onChange={(e) => setNewTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          className="h-8 text-sm flex-1 max-w-md"
        />
        <Button
          size="sm"
          onClick={handleAdd}
          disabled={!newTitle.trim()}
        >
          <Plus size={14} className="mr-1" /> Add
        </Button>
        <div className="ml-auto flex items-center gap-2">
          <button
            type="button"
            onClick={cycleDefaultVisibility}
            className="text-[11px] text-muted-foreground hover:text-foreground flex items-center gap-1"
            title="Toggle engagement-wide default for new cards"
          >
            <Settings2 size={12} />
            New cards default:{" "}
            <span
              className={
                defaultVisibility === "open-book"
                  ? "text-green-500"
                  : "text-amber-500"
              }
            >
              {defaultVisibility === "open-book"
                ? "open-book"
                : "private"}
            </span>
          </button>
        </div>
      </div>

      {/* Board */}
      <DndContext
        sensors={sensors}
        collisionDetection={closestCorners}
        onDragStart={onDragStart}
        onDragEnd={onDragEnd}
      >
        <div className="flex-1 overflow-auto p-3">
          <div className="flex gap-3 min-w-max h-full">
            {KANBAN_COLUMNS.map((col) => (
              <KanbanColumn
                key={col.status}
                column={col}
                tasks={columns[col.status] ?? []}
                onOpenTask={setOpenId}
              />
            ))}
          </div>
        </div>
        <DragOverlay>
          {activeTask ? (
            <div className="opacity-90 rotate-1">
              <TaskCard task={activeTask} onOpen={() => {}} />
            </div>
          ) : null}
        </DragOverlay>
      </DndContext>

      {openTask && (
        <Suspense
          fallback={
            <div className="fixed inset-y-0 right-0 w-96 bg-background border-l border-border animate-pulse" />
          }
        >
          <TaskDetailDrawer
            task={openTask}
            onClose={() => setOpenId(null)}
          />
        </Suspense>
      )}
    </div>
  );
}
