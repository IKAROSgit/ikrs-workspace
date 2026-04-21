import { useEffect, useMemo, useState } from "react";
import {
  collection,
  onSnapshot,
  query,
  where,
} from "firebase/firestore";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { X, Eye, EyeOff, Trash2, ExternalLink, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { db } from "@/lib/firebase";
import { useTasks } from "@/hooks/useTasks";
import { KANBAN_COLUMNS } from "./KanbanColumn";
import { cn } from "@/lib/utils";
import type { Task, TaskNote, TaskPriority, TaskStatus } from "@/types";

/**
 * Task detail drawer. Slides in from the right when a card is
 * opened. Contains everything the card face doesn't show:
 *  - Title + description (both editable, autosave on blur)
 *  - Status pills (quick-change without drag)
 *  - Priority p1 / p2 / p3 radio
 *  - Client-visible toggle (writes shareEvents audit)
 *  - Drive link attachments (paste URL, click to open)
 *  - Notes timeline — author, body, timestamp, client-visible dot
 *  - Note composer
 *
 * Lives as a standalone component so it can be lazy-loaded — the
 * initial TasksView chunk doesn't need react-markdown / Firestore
 * subcollection listeners until the user clicks a card.
 */
export function TaskDetailDrawer({
  task,
  onClose,
}: {
  task: Task;
  onClose: () => void;
}) {
  const {
    setTitle,
    setDescription,
    setPriority,
    changeStatus,
    setClientVisible,
    addNote,
    addDriveLink,
    removeDriveLink,
    remove,
  } = useTasks();

  const [title, setTitleLocal] = useState(task.title);
  const [desc, setDescLocal] = useState(task.description ?? "");
  const [noteBody, setNoteBody] = useState("");
  const [noteSending, setNoteSending] = useState(false);
  const [newLink, setNewLink] = useState("");
  const [notes, setNotes] = useState<TaskNote[]>([]);

  // Reset local state when switching cards.
  useEffect(() => {
    setTitleLocal(task.title);
    setDescLocal(task.description ?? "");
    setNoteBody("");
    setNewLink("");
  }, [task.id, task.title, task.description]);

  // Subscribe to this task's notes subcollection. taskNotes lives
  // at the top level with a taskId index — per Codex §B.4, embedded
  // arrays would blow up Task doc wire size on every keystroke.
  useEffect(() => {
    const q = query(
      collection(db, "taskNotes"),
      where("taskId", "==", task.id),
    );
    const unsub = onSnapshot(q, (snap) => {
      const rows = snap.docs.map(
        (d) => ({ ...d.data(), id: d.id }) as TaskNote,
      );
      rows.sort((a, b) => {
        const ta = toMillis(a.createdAt);
        const tb = toMillis(b.createdAt);
        return tb - ta; // newest first
      });
      setNotes(rows);
    });
    return unsub;
  }, [task.id]);

  const addAndClearNote = async () => {
    const body = noteBody.trim();
    if (!body) return;
    setNoteSending(true);
    try {
      await addNote(task.id, body);
      setNoteBody("");
    } finally {
      setNoteSending(false);
    }
  };

  const addAndClearLink = async () => {
    const url = newLink.trim();
    if (!url) return;
    if (!isLikelyDriveUrl(url)) {
      // Accept anything http-ish — we don't force Drive domain because
      // some consultants share via Dropbox / Notion / internal CMS.
      if (!/^https?:\/\//i.test(url)) {
        return;
      }
    }
    await addDriveLink(task.id, url);
    setNewLink("");
  };

  const clientVisibleResolved = task.clientVisible ?? true; // open-book default

  const priorities: TaskPriority[] = useMemo(() => ["p1", "p2", "p3"], []);

  return (
    <div className="fixed inset-y-0 right-0 z-40 w-full max-w-md bg-background border-l border-border shadow-2xl flex flex-col animate-in slide-in-from-right duration-200">
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-border">
        <h3 className="text-sm font-semibold truncate flex-1">Task</h3>
        <Button variant="ghost" size="sm" onClick={onClose} aria-label="Close">
          <X size={14} />
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="p-4 space-y-5 text-sm">
          {/* Title */}
          <div>
            <Input
              value={title}
              onChange={(e) => setTitleLocal(e.target.value)}
              onBlur={() => {
                if (title.trim() && title !== task.title) {
                  setTitle(task.id, title.trim());
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  (e.target as HTMLInputElement).blur();
                }
              }}
              className="text-base font-semibold h-10"
              placeholder="Task title"
            />
          </div>

          {/* Status pills — click to change */}
          <section>
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
              Status
            </div>
            <div className="flex flex-wrap gap-1.5">
              {KANBAN_COLUMNS.map((col) => (
                <button
                  type="button"
                  key={col.status}
                  onClick={() => changeStatus(task.id, col.status as TaskStatus)}
                  className={cn(
                    "text-[11px] px-2 py-1 rounded border transition-colors",
                    task.status === col.status
                      ? `${col.accent} border-current`
                      : "border-border text-muted-foreground hover:bg-muted",
                  )}
                >
                  {col.label}
                </button>
              ))}
            </div>
          </section>

          {/* Priority */}
          <section>
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
              Priority
            </div>
            <div className="flex gap-1">
              {priorities.map((p) => (
                <button
                  type="button"
                  key={p}
                  onClick={() => setPriority(task.id, p)}
                  className={cn(
                    "text-[11px] px-2 py-1 rounded border font-medium",
                    task.priority === p
                      ? priorityActive(p)
                      : "border-border text-muted-foreground hover:bg-muted",
                  )}
                >
                  {p.toUpperCase()}
                </button>
              ))}
            </div>
          </section>

          {/* Visibility */}
          <section>
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
              Client visibility
            </div>
            <button
              type="button"
              onClick={() => setClientVisible(task.id, !clientVisibleResolved)}
              className={cn(
                "flex items-center gap-2 text-xs px-3 py-2 rounded border w-full",
                clientVisibleResolved
                  ? "border-green-500/40 bg-green-500/10 text-green-400"
                  : "border-border bg-muted text-muted-foreground",
              )}
            >
              {clientVisibleResolved ? <Eye size={14} /> : <EyeOff size={14} />}
              <span className="flex-1 text-left">
                {clientVisibleResolved
                  ? "Visible to client"
                  : "Private to consultant"}
              </span>
              <span className="text-[10px]">click to toggle</span>
            </button>
            <p className="text-[10px] text-muted-foreground mt-1">
              Every flip is recorded in the audit trail.
            </p>
          </section>

          {/* Description */}
          <section>
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
              Description
            </div>
            <textarea
              value={desc}
              onChange={(e) => setDescLocal(e.target.value)}
              onBlur={() => {
                if (desc !== (task.description ?? "")) {
                  setDescription(task.id, desc);
                }
              }}
              placeholder="Context for this task…"
              className="w-full min-h-[80px] text-sm p-2 bg-background border border-border rounded-md resize-y focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </section>

          {/* Drive / attachment links */}
          <section>
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
              Links
            </div>
            {(task.driveLinks ?? []).length === 0 ? (
              <p className="text-xs text-muted-foreground">No links yet.</p>
            ) : (
              <ul className="space-y-1 mb-2">
                {(task.driveLinks ?? []).map((url) => (
                  <li
                    key={url}
                    className="flex items-center gap-2 text-xs"
                    title={url}
                  >
                    <a
                      href={url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex-1 truncate text-blue-500 hover:underline"
                    >
                      {displayUrl(url)}
                    </a>
                    <button
                      type="button"
                      onClick={() => removeDriveLink(task.id, url)}
                      className="text-muted-foreground hover:text-destructive"
                      aria-label="Remove link"
                    >
                      <Trash2 size={12} />
                    </button>
                    <ExternalLink size={11} className="text-muted-foreground" />
                  </li>
                ))}
              </ul>
            )}
            <div className="flex gap-1">
              <Input
                value={newLink}
                onChange={(e) => setNewLink(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addAndClearLink()}
                placeholder="Paste a Drive (or any) URL"
                className="h-8 text-xs flex-1"
              />
              <Button
                size="sm"
                variant="outline"
                onClick={addAndClearLink}
                disabled={!newLink.trim()}
              >
                <Plus size={12} />
              </Button>
            </div>
          </section>

          {/* Notes timeline */}
          <section>
            <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
              Notes ({notes.length})
            </div>
            <div className="space-y-2 mb-2">
              <textarea
                value={noteBody}
                onChange={(e) => setNoteBody(e.target.value)}
                placeholder="Add a note…"
                className="w-full min-h-[60px] text-xs p-2 bg-background border border-border rounded-md resize-y focus:outline-none focus:ring-2 focus:ring-primary"
                disabled={noteSending}
              />
              <Button
                size="sm"
                onClick={addAndClearNote}
                disabled={!noteBody.trim() || noteSending}
              >
                {noteSending ? "Adding…" : "Add note"}
              </Button>
            </div>
            {notes.length === 0 ? (
              <p className="text-xs text-muted-foreground">No notes yet.</p>
            ) : (
              <ul className="space-y-2">
                {notes.map((n) => (
                  <li
                    key={n.id}
                    className="border-l-2 border-border pl-2 text-xs"
                  >
                    <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
                      <span className="font-medium">
                        {n.authorKind === "consultant"
                          ? "you"
                          : n.authorKind}
                      </span>
                      <span>·</span>
                      <span>{formatDateTime(toMillis(n.createdAt))}</span>
                      {n.clientVisible === false && (
                        <>
                          <span>·</span>
                          <EyeOff size={9} />
                        </>
                      )}
                    </div>
                    <div className="prose prose-sm dark:prose-invert max-w-none mt-1 text-xs">
                      <ReactMarkdown remarkPlugins={[remarkGfm]}>
                        {n.body}
                      </ReactMarkdown>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {/* Danger */}
          <section className="pt-4 border-t border-border">
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                if (
                  window.confirm("Delete this task? This can't be undone.")
                ) {
                  remove(task.id).then(onClose);
                }
              }}
              className="text-destructive hover:text-destructive hover:bg-destructive/10"
            >
              <Trash2 size={12} className="mr-1.5" />
              Delete task
            </Button>
          </section>
        </div>
      </ScrollArea>
    </div>
  );
}

function priorityActive(p: TaskPriority): string {
  switch (p) {
    case "p1":
      return "border-red-500/60 bg-red-500/15 text-red-400";
    case "p2":
      return "border-yellow-500/60 bg-yellow-500/15 text-yellow-400";
    case "p3":
      return "border-blue-500/60 bg-blue-500/15 text-blue-400";
  }
}

function toMillis(d: unknown): number {
  if (!d) return 0;
  if (d instanceof Date) return d.getTime();
  const obj = d as { seconds?: number; toMillis?: () => number };
  if (typeof obj.toMillis === "function") return obj.toMillis();
  if (typeof obj.seconds === "number") return obj.seconds * 1000;
  if (typeof d === "string") return new Date(d).getTime();
  return 0;
}

function formatDateTime(ms: number): string {
  if (!ms) return "";
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function displayUrl(raw: string): string {
  try {
    const u = new URL(raw);
    const path = u.pathname.length > 1 ? u.pathname : "";
    return `${u.hostname}${path}`.slice(0, 80);
  } catch {
    return raw.slice(0, 80);
  }
}

function isLikelyDriveUrl(u: string): boolean {
  return /drive\.google\.com|docs\.google\.com/.test(u);
}
