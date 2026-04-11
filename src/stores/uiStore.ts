import { create } from "zustand";
import type { ViewId } from "@/Router";
import type { Theme } from "@/types";

interface UiState {
  activeView: ViewId;
  theme: Theme;
  sideRailCollapsed: boolean;
  setActiveView: (view: ViewId) => void;
  setTheme: (theme: Theme) => void;
  toggleSideRail: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeView: "inbox",
  theme: "dark",
  sideRailCollapsed: false,
  setActiveView: (view) => set({ activeView: view }),
  setTheme: (theme) => set({ theme }),
  toggleSideRail: () => set((state) => ({ sideRailCollapsed: !state.sideRailCollapsed })),
}));
