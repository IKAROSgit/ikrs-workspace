import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/**
 * Max-subscription usage ledger, persisted in localStorage.
 *
 * Claude Code reports a per-turn `costUsd` derived from
 * pay-per-token pricing. We accumulate it here so the session
 * details panel can answer "how much did I use today?" and
 * "how much has this engagement cost all-time?" — the two rollups
 * consultants were asking for after the per-session number on the
 * indicator proved ambiguous (each reconnect starts a fresh
 * counter, so the visible number was always smaller than reality).
 *
 * These are usage *meters*, not bills — Moe's Max subscription
 * absorbs usage up to a monthly quota. Surfacing the numbers lets
 * him judge whether a given engagement is burning through
 * disproportionate quota before the monthly alert fires.
 */

function todayKey(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

interface EngagementLedger {
  /** `YYYY-MM-DD` → total cost for that day */
  byDay: Record<string, number>;
  /** Sum over all days — cached so the panel doesn't recompute on every read. */
  allTime: number;
}

interface CostLedgerState {
  engagements: Record<string, EngagementLedger>;
  addCost: (engagementId: string, usd: number) => void;
  todayTotal: () => number;
  engagementTodayTotal: (engagementId: string) => number;
  engagementAllTimeTotal: (engagementId: string) => number;
  reset: () => void;
}

export const useCostLedgerStore = create<CostLedgerState>()(
  persist(
    (set, get) => ({
      engagements: {},

      addCost: (engagementId, usd) => {
        if (!engagementId || usd <= 0) return;
        set((s) => {
          const key = todayKey();
          const current = s.engagements[engagementId] ?? {
            byDay: {},
            allTime: 0,
          };
          const nextDay = (current.byDay[key] ?? 0) + usd;
          return {
            engagements: {
              ...s.engagements,
              [engagementId]: {
                byDay: { ...current.byDay, [key]: nextDay },
                allTime: current.allTime + usd,
              },
            },
          };
        });
      },

      todayTotal: () => {
        const key = todayKey();
        const entries = Object.values(get().engagements);
        let sum = 0;
        for (const e of entries) sum += e.byDay[key] ?? 0;
        return sum;
      },

      engagementTodayTotal: (engagementId) => {
        const key = todayKey();
        return get().engagements[engagementId]?.byDay[key] ?? 0;
      },

      engagementAllTimeTotal: (engagementId) => {
        return get().engagements[engagementId]?.allTime ?? 0;
      },

      reset: () => set({ engagements: {} }),
    }),
    {
      name: "ikrs-cost-ledger",
      storage: createJSONStorage(() => localStorage),
      version: 1,
      partialize: (s) => ({ engagements: s.engagements }),
    },
  ),
);
