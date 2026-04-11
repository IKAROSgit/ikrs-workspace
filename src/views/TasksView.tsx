import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Trash2, Circle, CircleDot, CheckCircle2 } from "lucide-react";
import { useTasks } from "@/hooks/useTasks";
import { useEngagementStore } from "@/stores/engagementStore";
import type { Task, TaskStatus } from "@/types";

const STATUS_ICON: Record<TaskStatus, typeof Circle> = {
  todo: Circle,
  in_progress: CircleDot,
  done: CheckCircle2,
};

const PRIORITY_COLOR: Record<string, string> = {
  p1: "bg-red-500/20 text-red-400",
  p2: "bg-yellow-500/20 text-yellow-400",
  p3: "bg-blue-500/20 text-blue-400",
};

function TaskRow({ task, onToggle, onDelete }: {
  task: Task;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const Icon = STATUS_ICON[task.status];
  return (
    <div className="flex items-center gap-3 py-2 px-3 rounded-lg hover:bg-accent/50 group">
      <button type="button" onClick={onToggle} aria-label={`Toggle ${task.title}`}>
        <Icon size={18} className={task.status === "done" ? "text-green-500" : "text-muted-foreground"} />
      </button>
      <span className={`flex-1 text-sm ${task.status === "done" ? "line-through text-muted-foreground" : ""}`}>
        {task.title}
      </span>
      <Badge className={PRIORITY_COLOR[task.priority]}>
        {task.priority}
      </Badge>
      {task.tags.map((tag) => (
        <Badge key={tag} variant="outline" className="text-xs">{tag}</Badge>
      ))}
      <button
        type="button"
        onClick={onDelete}
        className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive"
        aria-label={`Delete ${task.title}`}
      >
        <Trash2 size={14} />
      </button>
    </div>
  );
}

export default function TasksView() {
  const { tasks, addTask, toggleStatus, remove } = useTasks();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const [newTaskTitle, setNewTaskTitle] = useState("");

  if (!activeEngagementId) {
    return (
      <div className="flex flex-col h-full p-4">
        <h2 className="text-lg font-semibold mb-4">Tasks</h2>
        <p className="text-muted-foreground">Select an engagement to manage tasks.</p>
      </div>
    );
  }

  const handleAdd = async () => {
    if (!newTaskTitle.trim()) return;
    await addTask(newTaskTitle.trim());
    setNewTaskTitle("");
  };

  const sections: { label: string; status: TaskStatus }[] = [
    { label: "To Do", status: "todo" },
    { label: "In Progress", status: "in_progress" },
    { label: "Done", status: "done" },
  ];

  return (
    <div className="flex flex-col h-full p-4">
      <div className="flex items-center gap-2 mb-4">
        <Input
          placeholder="Add a task..."
          value={newTaskTitle}
          onChange={(e) => setNewTaskTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          className="flex-1"
        />
        <Button onClick={handleAdd} disabled={!newTaskTitle.trim()}>Add</Button>
      </div>

      {sections.map(({ label, status }) => {
        const sectionTasks = tasks.filter((t) => t.status === status);
        if (sectionTasks.length === 0 && status !== "todo") return null;
        return (
          <div key={status} className="mb-4">
            <h3 className="text-sm font-medium text-muted-foreground mb-2">{label} ({sectionTasks.length})</h3>
            {sectionTasks.map((task) => (
              <TaskRow
                key={task.id}
                task={task}
                onToggle={() => toggleStatus(task)}
                onDelete={() => remove(task.id)}
              />
            ))}
          </div>
        );
      })}
    </div>
  );
}
