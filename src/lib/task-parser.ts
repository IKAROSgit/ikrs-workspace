import type { Task, TaskStatus, TaskPriority, Subtask } from "@/types";

const CHECKBOX_RE = /^- \[([ x/])\] \*\*(.+?)\*\*(.*)$/;
const SUBTASK_RE = /^  - \[([ x])\] (.+)$/;
const PRIORITY_RE = /`(p[123])`/;
const TAG_RE = /`(#[a-zA-Z0-9_-]+)`/g;
const DUE_RE = /`due:(\d{4}-\d{2}-\d{2})`/;

// Mapping between markdown-checkbox syntax and the Kanban status
// vocabulary. Legacy `todo` folds into `backlog` post-2026-04-21.
// "awaiting_client", "blocked", "in_review" aren't expressible in
// plain-markdown checkbox state — those transitions happen in the
// app UI or via frontmatter in a structured task file.
function checkboxToStatus(char: string): TaskStatus {
  if (char === "x") return "done";
  if (char === "/") return "in_progress";
  return "backlog";
}

function statusToCheckbox(status: TaskStatus): string {
  if (status === "done") return "x";
  if (status === "in_progress" || status === "in_review") return "/";
  // backlog, awaiting_client, blocked all render as an empty checkbox
  // in the legacy markdown round-trip.
  return " ";
}

export function parseTasksMd(
  md: string,
  engagementId: string
): Omit<Task, "id" | "createdAt" | "updatedAt">[] {
  const lines = md.split("\n");
  const tasks: Omit<Task, "id" | "createdAt" | "updatedAt">[] = [];
  let sortOrder = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const match = line.match(CHECKBOX_RE);
    if (!match) continue;

    const checkbox = match[1] ?? " ";
    const title = match[2] ?? "";
    const meta = match[3] ?? "";
    const status = checkboxToStatus(checkbox);
    const priorityMatch = meta.match(PRIORITY_RE);
    const priority: TaskPriority = (priorityMatch?.[1] as TaskPriority) ?? "p2";
    const tags: string[] = [];
    let tagMatch: RegExpExecArray | null;
    const tagRe = new RegExp(TAG_RE.source, "g");
    while ((tagMatch = tagRe.exec(meta)) !== null) {
      const tag = tagMatch[1];
      if (tag !== undefined) tags.push(tag);
    }
    const dueMatch = meta.match(DUE_RE);
    const dueDate = dueMatch?.[1] !== undefined ? new Date(dueMatch[1]) : undefined;

    const subtasks: Subtask[] = [];
    while (i + 1 < lines.length) {
      const nextLine = lines[i + 1] ?? "";
      const subMatch = nextLine.match(SUBTASK_RE);
      if (!subMatch) break;
      subtasks.push({
        title: subMatch[2] ?? "",
        done: subMatch[1] === "x",
      });
      i++;
    }

    tasks.push({
      _v: 1,
      engagementId,
      title,
      status,
      priority,
      dueDate,
      tags,
      subtasks,
      sortOrder: sortOrder++,
      source: "imported",
    });
  }

  return tasks;
}

export function renderTasksMd(
  tasks: Pick<
    Task,
    "title" | "status" | "priority" | "dueDate" | "tags" | "subtasks"
  >[],
  clientName: string
): string {
  const sections: Record<TaskStatus, typeof tasks> = {
    backlog: [],
    in_progress: [],
    awaiting_client: [],
    blocked: [],
    in_review: [],
    done: [],
  };

  for (const task of tasks) {
    sections[task.status].push(task);
  }

  const lines: string[] = [`# ${clientName} — Tasks`, ""];

  // Markdown export folds the 6-column Kanban into the 3 classic
  // sections ('To Do' / 'In Progress' / 'Done') for compatibility
  // with the vault's existing Obsidian-style tasks file. Richer
  // status info lives in the Firestore source of truth.
  const sectionOrder: { heading: string; statuses: TaskStatus[] }[] = [
    {
      heading: "## To Do",
      statuses: ["backlog", "awaiting_client", "blocked"],
    },
    {
      heading: "## In Progress",
      statuses: ["in_progress", "in_review"],
    },
    {
      heading: "## Done",
      statuses: ["done"],
    },
  ];

  for (const { heading, statuses } of sectionOrder) {
    const sectionTasks = statuses.flatMap((s) => sections[s]);
    if (sectionTasks.length === 0) continue;

    lines.push(heading, "");
    for (const task of sectionTasks) {
      const cb = statusToCheckbox(task.status);
      let meta = ` \`${task.priority}\``;
      for (const tag of task.tags) {
        meta += ` \`${tag}\``;
      }
      if (task.dueDate) {
        const d = task.dueDate;
        const yyyy = d.getFullYear();
        const mm = String(d.getMonth() + 1).padStart(2, "0");
        const dd = String(d.getDate()).padStart(2, "0");
        meta += ` \`due:${yyyy}-${mm}-${dd}\``;
      }
      lines.push(`- [${cb}] **${task.title}**${meta}`);
      for (const sub of task.subtasks) {
        const subCb = sub.done ? "x" : " ";
        lines.push(`  - [${subCb}] ${sub.title}`);
      }
    }
    lines.push("");
  }

  return lines.join("\n");
}
