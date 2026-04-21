import { createContext, useContext, useEffect, type ReactNode } from "react";
import {
  collection,
  query,
  where,
  onSnapshot,
  doc,
  addDoc,
  updateDoc,
  deleteDoc,
  serverTimestamp,
} from "firebase/firestore";
import { db } from "@/lib/firebase";
import { useAuth } from "@/providers/AuthProvider";
import { useEngagementStore } from "@/stores/engagementStore";
import { useTaskStore } from "@/stores/taskStore";
import type { Engagement, Client, Task, TaskStatus } from "@/types";

interface EngagementContextValue {
  createClient: (data: Omit<Client, "id" | "createdAt" | "updatedAt" | "_v">) => Promise<string>;
  createEngagement: (data: Omit<Engagement, "id" | "createdAt" | "updatedAt" | "_v">) => Promise<string>;
  updateEngagement: (id: string, data: Partial<Engagement>) => Promise<void>;
  deleteEngagement: (id: string) => Promise<void>;
  createTask: (data: Omit<Task, "id" | "createdAt" | "updatedAt" | "_v">) => Promise<string>;
  updateTask: (id: string, data: Partial<Task>) => Promise<void>;
  deleteTask: (id: string) => Promise<void>;
  /** Add a note to a task. Creates a taskNotes doc + optimistically
   *  bumps the task's notesCount denormalised field. */
  addTaskNote: (
    taskId: string,
    body: string,
    authorKind?: "consultant" | "claude" | "client",
    clientVisible?: boolean,
  ) => Promise<string>;
  /** Flip a card's clientVisible. Writes a shareEvents audit doc. */
  setTaskClientVisible: (taskId: string, visible: boolean) => Promise<void>;
  /** Change task status; also writes shareEvents when the transition
   *  is to/from a client-visible state per Codex §B.8. */
  changeTaskStatus: (taskId: string, nextStatus: TaskStatus) => Promise<void>;
}

const EngagementContext = createContext<EngagementContextValue | null>(null);

export function useEngagementActions() {
  const ctx = useContext(EngagementContext);
  if (!ctx) throw new Error("useEngagementActions must be used within EngagementProvider");
  return ctx;
}

export function EngagementProvider({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const setEngagements = useEngagementStore((s) => s.setEngagements);
  const setClients = useEngagementStore((s) => s.setClients);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const setTasks = useTaskStore((s) => s.setTasks);

  useEffect(() => {
    if (!user) return;
    const q = query(collection(db, "engagements"), where("consultantId", "==", user.uid));
    const unsubscribe = onSnapshot(q, (snap) => {
      const engagements = snap.docs.map((d) => ({ ...d.data(), id: d.id }) as Engagement);
      setEngagements(engagements);
    });
    return unsubscribe;
  }, [user, setEngagements]);

  useEffect(() => {
    if (!user) return;
    const unsubscribe = onSnapshot(collection(db, "clients"), (snap) => {
      const clients = snap.docs.map((d) => ({ ...d.data(), id: d.id }) as Client);
      setClients(clients);
    });
    return unsubscribe;
  }, [user, setClients]);

  useEffect(() => {
    if (!activeEngagementId) {
      setTasks([]);
      return;
    }
    const q = query(collection(db, "ikrs_tasks"), where("engagementId", "==", activeEngagementId));
    const unsubscribe = onSnapshot(q, (snap) => {
      const tasks = snap.docs.map((d) => {
        const raw = { ...d.data(), id: d.id } as Task;
        // Read-time migration from v1 status vocabulary (todo /
        // in_progress / done) to v2 (backlog / in_progress /
        // awaiting_client / blocked / in_review / done). Codex
        // 2026-04-21 §B.1: map `todo → backlog`, leave others alone,
        // lazy-fire an update so next read is clean. Don't block on
        // the write — Firestore SDK queues offline.
        const asAny = raw as unknown as { status: string; _v?: number };
        if (asAny.status === "todo") {
          const migrated = {
            ...raw,
            status: "backlog" as TaskStatus,
            _v: 2 as const,
          };
          void updateDoc(doc(db, "ikrs_tasks", raw.id), {
            status: "backlog",
            _v: 2,
            updatedAt: serverTimestamp(),
          }).catch((e) => {
            // Non-fatal — next user write will carry the new value.
            // eslint-disable-next-line no-console
            console.debug("[task migration] lazy update failed", e);
          });
          return migrated;
        }
        return raw;
      });
      setTasks(tasks.sort((a, b) => a.sortOrder - b.sortOrder));
    });
    return unsubscribe;
  }, [activeEngagementId, setTasks]);

  const actions: EngagementContextValue = {
    createClient: async (data) => {
      const ref = await addDoc(collection(db, "clients"), {
        ...data,
        _v: 1,
        createdAt: serverTimestamp(),
        updatedAt: serverTimestamp(),
      });
      return ref.id;
    },
    createEngagement: async (data) => {
      const ref = await addDoc(collection(db, "engagements"), {
        ...data,
        _v: 1,
        createdAt: serverTimestamp(),
        updatedAt: serverTimestamp(),
      });
      return ref.id;
    },
    updateEngagement: async (id, data) => {
      await updateDoc(doc(db, "engagements", id), {
        ...data,
        updatedAt: serverTimestamp(),
      });
    },
    deleteEngagement: async (id) => {
      await deleteDoc(doc(db, "engagements", id));
    },
    createTask: async (data) => {
      const ref = await addDoc(collection(db, "ikrs_tasks"), {
        ...data,
        _v: 1,
        createdAt: serverTimestamp(),
        updatedAt: serverTimestamp(),
      });
      return ref.id;
    },
    updateTask: async (id, data) => {
      await updateDoc(doc(db, "ikrs_tasks", id), {
        ...data,
        updatedAt: serverTimestamp(),
      });
    },
    deleteTask: async (id) => {
      await deleteDoc(doc(db, "ikrs_tasks", id));
    },

    addTaskNote: async (taskId, body, authorKind = "consultant", clientVisible) => {
      if (!user) throw new Error("Not signed in");
      const engagementId =
        useTaskStore.getState().tasks.find((t) => t.id === taskId)?.engagementId;
      if (!engagementId) throw new Error("Task not found");
      // Inherit clientVisible from the task if not specified; default
      // to the task's flag (which may itself inherit from engagement).
      const effectiveVisible =
        clientVisible ??
        useTaskStore.getState().tasks.find((t) => t.id === taskId)?.clientVisible ??
        true;
      const ref = await addDoc(collection(db, "taskNotes"), {
        _v: 1,
        taskId,
        engagementId,
        authorKind,
        authorId: authorKind === "consultant" ? user.uid : authorKind,
        body,
        clientVisible: effectiveVisible,
        createdAt: serverTimestamp(),
        updatedAt: serverTimestamp(),
      });
      // Bump denormalised count on the task (best-effort).
      const current =
        useTaskStore.getState().tasks.find((t) => t.id === taskId)?.notesCount ?? 0;
      void updateDoc(doc(db, "ikrs_tasks", taskId), {
        notesCount: current + 1,
        updatedAt: serverTimestamp(),
      });
      return ref.id;
    },

    setTaskClientVisible: async (taskId, visible) => {
      if (!user) throw new Error("Not signed in");
      const task = useTaskStore.getState().tasks.find((t) => t.id === taskId);
      if (!task) throw new Error("Task not found");
      const prev = task.clientVisible ?? null;
      if (prev === visible) return;
      // Append-only audit event BEFORE the task update, so if the
      // task update fails the audit still captured intent.
      await addDoc(
        collection(db, "ikrs_tasks", taskId, "shareEvents"),
        {
          _v: 1,
          taskId,
          engagementId: task.engagementId,
          by: "consultant",
          byId: user.uid,
          field: "clientVisible",
          from: prev,
          to: visible,
          timestamp: serverTimestamp(),
        },
      );
      const patch: Partial<Task> = { clientVisible: visible };
      if (visible) patch.sharedAt = new Date();
      await updateDoc(doc(db, "ikrs_tasks", taskId), {
        ...patch,
        updatedAt: serverTimestamp(),
      });
    },

    changeTaskStatus: async (taskId, nextStatus) => {
      if (!user) throw new Error("Not signed in");
      const task = useTaskStore.getState().tasks.find((t) => t.id === taskId);
      if (!task) throw new Error("Task not found");
      if (task.status === nextStatus) return;
      // Audit status transitions to/from "done" or "in_review" —
      // client-meaningful transitions per Codex §B.8.
      const isClientMeaningful = (s: TaskStatus) =>
        s === "done" || s === "in_review";
      if (isClientMeaningful(task.status) || isClientMeaningful(nextStatus)) {
        await addDoc(
          collection(db, "ikrs_tasks", taskId, "shareEvents"),
          {
            _v: 1,
            taskId,
            engagementId: task.engagementId,
            by: "consultant",
            byId: user.uid,
            field: "status",
            from: task.status,
            to: nextStatus,
            timestamp: serverTimestamp(),
          },
        );
      }
      await updateDoc(doc(db, "ikrs_tasks", taskId), {
        status: nextStatus,
        updatedAt: serverTimestamp(),
      });
    },
  };

  return (
    <EngagementContext.Provider value={actions}>
      {children}
    </EngagementContext.Provider>
  );
}
