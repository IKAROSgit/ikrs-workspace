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
import type { Engagement, Client, Task } from "@/types";

interface EngagementContextValue {
  createClient: (data: Omit<Client, "id" | "createdAt" | "updatedAt" | "_v">) => Promise<string>;
  createEngagement: (data: Omit<Engagement, "id" | "createdAt" | "updatedAt" | "_v">) => Promise<string>;
  updateEngagement: (id: string, data: Partial<Engagement>) => Promise<void>;
  deleteEngagement: (id: string) => Promise<void>;
  createTask: (data: Omit<Task, "id" | "createdAt" | "updatedAt" | "_v">) => Promise<string>;
  updateTask: (id: string, data: Partial<Task>) => Promise<void>;
  deleteTask: (id: string) => Promise<void>;
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
      const tasks = snap.docs.map((d) => ({ ...d.data(), id: d.id }) as Task);
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
  };

  return (
    <EngagementContext.Provider value={actions}>
      {children}
    </EngagementContext.Provider>
  );
}
