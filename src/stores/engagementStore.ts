import { create } from "zustand";
import type { Engagement, Client } from "@/types";

interface EngagementState {
  engagements: Engagement[];
  clients: Client[];
  activeEngagementId: string | null;
  // CODEX MUST-FIX #2: No getter — use derived selector in components instead
  setActiveEngagement: (id: string | null) => void;
  addEngagement: (engagement: Engagement) => void;
  updateEngagement: (id: string, updates: Partial<Engagement>) => void;
  removeEngagement: (id: string) => void;
  setEngagements: (engagements: Engagement[]) => void;
  addClient: (client: Client) => void;
  setClients: (clients: Client[]) => void;
}

// CODEX MUST-FIX #2: No getter properties on Zustand stores — they are not reactive.
// Use derived selectors in components instead: useEngagementStore((s) => s.engagements.find(e => e.id === s.activeEngagementId))
export const useEngagementStore = create<EngagementState>((set) => ({
  engagements: [],
  clients: [],
  activeEngagementId: null,
  setActiveEngagement: (id) => set({ activeEngagementId: id }),
  addEngagement: (engagement) =>
    set((state) => ({ engagements: [...state.engagements, engagement] })),
  updateEngagement: (id, updates) =>
    set((state) => ({
      engagements: state.engagements.map((e) =>
        e.id === id ? { ...e, ...updates, updatedAt: new Date() } : e
      ),
    })),
  removeEngagement: (id) =>
    set((state) => ({
      engagements: state.engagements.filter((e) => e.id !== id),
      activeEngagementId: state.activeEngagementId === id ? null : state.activeEngagementId,
    })),
  setEngagements: (engagements) => set({ engagements }),
  addClient: (client) =>
    set((state) => ({ clients: [...state.clients, client] })),
  setClients: (clients) => set({ clients }),
}));
