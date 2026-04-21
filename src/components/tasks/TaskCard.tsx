import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  Circle,
  MessageSquare,
  Calendar,
  Eye,
  EyeOff,
  Bot,
  User,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { Task } from "@/types";

const PRIORITY_COLOR: Record<string, string> = {
  p1: "text-red-400 border-red-500/30",
  p2: "text-yellow-400 border-yellow-500/30",
  p3: "text-blue-400 border-blue-500/30",
};

const ASSIGNEE_ICON = {
  consultant: User,
  claude: Bot,
  client: User,
} as const;

/**
 * Kanban card face. Memoized via React.memo at the board level so
 * a neighbour's drag doesn't re-render every card. Card face is
 * density-optimised for a 1280px viewport: ~180px column = tight
 * truncation on title, single-line meta row underneath.
 *
 * Tap = opens the detail drawer (bubbles click to parent via
 * onOpen). Drag = @dnd-kit's sortable; the `{...listeners}` below
 * is applied to the whole card face — every pixel is a drag
 * handle. Click detection uses dnd-kit's activation constraint on
 * the parent DndContext (distance 8px).
 */
export function TaskCard({
  task,
  onOpen,
}: {
  task: Task;
  onOpen: (taskId: string) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: task.id, data: { type: "task", status: task.status } });

  const style = {
    transform: CSS.Translate.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : 1,
  };

  const AssigneeIcon = ASSIGNEE_ICON[task.assignee ?? "consultant"];
  const due = task.dueDate
    ? task.dueDate instanceof Date
      ? task.dueDate
      : new Date((task.dueDate as unknown as { seconds?: number }).seconds
          ? (task.dueDate as unknown as { seconds: number }).seconds * 1000
          : (task.dueDate as unknown as string))
    : undefined;

  return (
    <button
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      onClick={() => onOpen(task.id)}
      type="button"
      className={cn(
        "w-full text-left rounded-md border border-border bg-card p-2.5 text-xs shadow-sm",
        "hover:border-primary/40 hover:shadow-md transition-colors",
        "cursor-grab active:cursor-grabbing",
        isDragging && "ring-2 ring-primary",
      )}
    >
      <div className="flex items-start gap-2">
        <Circle
          size={10}
          className={cn(
            "mt-0.5 flex-shrink-0 fill-current",
            PRIORITY_COLOR[task.priority]?.split(" ")[0] ?? "text-muted-foreground",
          )}
          aria-label={task.priority}
        />
        <span className="flex-1 text-sm font-medium line-clamp-2 leading-tight">
          {task.title}
        </span>
        {task.clientVisible === false && (
          <EyeOff
            size={12}
            className="mt-0.5 text-muted-foreground flex-shrink-0"
            aria-label="Private"
          />
        )}
        {task.clientVisible === true && (
          <Eye
            size={12}
            className="mt-0.5 text-green-500 flex-shrink-0"
            aria-label="Visible to client"
          />
        )}
      </div>
      <div className="mt-2 flex items-center gap-3 text-[11px] text-muted-foreground">
        <AssigneeIcon size={11} aria-label={task.assignee ?? "consultant"} />
        {task.notesCount && task.notesCount > 0 ? (
          <span className="flex items-center gap-1">
            <MessageSquare size={11} />
            {task.notesCount}
          </span>
        ) : null}
        {due && (
          <span className="flex items-center gap-1">
            <Calendar size={11} />
            {formatDue(due)}
          </span>
        )}
        {task.tags?.slice(0, 2).map((t) => (
          <span
            key={t}
            className="px-1 py-0.5 rounded bg-muted text-[10px] truncate max-w-[64px]"
          >
            {t}
          </span>
        ))}
      </div>
    </button>
  );
}

/** Humanised date: today = "Today", tomorrow = "Tmrw", this week
 *  = day name, later = short month/day. Keeps the card face
 *  scannable rather than dumping a full ISO string. */
function formatDue(d: Date): string {
  if (Number.isNaN(d.getTime())) return "";
  const now = new Date();
  const midnight = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const daysDiff = Math.round(
    (midnight(d) - midnight(now)) / (1000 * 60 * 60 * 24),
  );
  if (daysDiff === 0) return "Today";
  if (daysDiff === 1) return "Tmrw";
  if (daysDiff === -1) return "Yday";
  if (daysDiff > 1 && daysDiff <= 6) {
    return d.toLocaleDateString(undefined, { weekday: "short" });
  }
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}
