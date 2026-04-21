import { SortableContext, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { useDroppable } from "@dnd-kit/core";
import type { Task, TaskStatus } from "@/types";
import { TaskCard } from "./TaskCard";
import { cn } from "@/lib/utils";

export interface KanbanColumnDef {
  status: TaskStatus;
  label: string;
  /** Tailwind accent colour for the column header badge. Keeps the
   *  board scannable — each status has an identity at a glance. */
  accent: string;
}

/** Centralised 6-column taxonomy. Shared between board + palette +
 *  Rust vault-parse so everyone agrees on the order. */
export const KANBAN_COLUMNS: KanbanColumnDef[] = [
  { status: "backlog", label: "Backlog", accent: "bg-slate-500/15 text-slate-300" },
  { status: "in_progress", label: "In progress", accent: "bg-blue-500/15 text-blue-300" },
  { status: "awaiting_client", label: "Awaiting client", accent: "bg-amber-500/15 text-amber-300" },
  { status: "blocked", label: "Blocked", accent: "bg-red-500/15 text-red-300" },
  { status: "in_review", label: "In review", accent: "bg-violet-500/15 text-violet-300" },
  { status: "done", label: "Done", accent: "bg-green-500/15 text-green-300" },
];

export function KanbanColumn({
  column,
  tasks,
  onOpenTask,
}: {
  column: KanbanColumnDef;
  tasks: Task[];
  onOpenTask: (taskId: string) => void;
}) {
  // Droppable zone covers the column body — even when empty we
  // still accept drops via setNodeRef so a card can land in an
  // empty column.
  const { setNodeRef, isOver } = useDroppable({
    id: `column-${column.status}`,
    data: { type: "column", status: column.status },
  });

  return (
    <div
      className={cn(
        "flex flex-col min-w-[200px] flex-1 rounded-lg border border-border bg-muted/20",
        "max-h-full",
      )}
    >
      <div className="flex items-center justify-between px-3 py-2 border-b border-border sticky top-0 bg-muted/30 backdrop-blur-sm rounded-t-lg">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "text-[10px] uppercase tracking-wider font-semibold px-1.5 py-0.5 rounded",
              column.accent,
            )}
          >
            {column.label}
          </span>
          <span className="text-xs text-muted-foreground">{tasks.length}</span>
        </div>
      </div>
      <div
        ref={setNodeRef}
        className={cn(
          "flex-1 overflow-y-auto p-2 space-y-2 transition-colors",
          isOver && "bg-primary/5",
        )}
      >
        <SortableContext
          items={tasks.map((t) => t.id)}
          strategy={verticalListSortingStrategy}
        >
          {tasks.map((t) => (
            <TaskCard key={t.id} task={t} onOpen={onOpenTask} />
          ))}
        </SortableContext>
        {tasks.length === 0 && (
          <div className="h-24 flex items-center justify-center text-xs text-muted-foreground">
            Drop cards here
          </div>
        )}
      </div>
    </div>
  );
}
