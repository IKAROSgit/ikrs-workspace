import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";

/**
 * Per-view resizable panel widths, persisted across restarts.
 *
 * Completes Moe's "the whole view is adaptive, dynamic, resizable"
 * ask (2026-04-21) without a shell-level refactor: each view with
 * an internal two-pane layout wraps its body in
 * `<PanelGroup>` / `<Panel>` / `<PanelResizeHandle>` and feeds
 * sizes through this store.
 *
 * Sizes are percentages (0-100) of the view body, not pixels —
 * resolution changes don't produce slivers.
 *
 * Each view key is a distinct setting: ChatView's ideal split
 * isn't NotesView's ideal split.
 */
export type PanelViewKey =
  | "chat"
  | "notes"
  | "calendar"
  | "tasks";

interface DeskspaceState {
  /** `{ [viewKey]: [leftPercent, rightPercent] }` */
  panelSizes: Record<PanelViewKey, [number, number]>;
  setPanelSizes: (view: PanelViewKey, sizes: [number, number]) => void;
  reset: () => void;
}

const DEFAULT_SIZES: Record<PanelViewKey, [number, number]> = {
  chat: [68, 32],
  notes: [40, 60],
  calendar: [60, 40],
  tasks: [70, 30],
};

export const useDeskspaceStore = create<DeskspaceState>()(
  persist(
    (set) => ({
      panelSizes: DEFAULT_SIZES,
      setPanelSizes: (view, sizes) =>
        set((s) => ({
          panelSizes: { ...s.panelSizes, [view]: sizes },
        })),
      reset: () => set({ panelSizes: DEFAULT_SIZES }),
    }),
    {
      name: "ikrs-deskspace-sizes",
      storage: createJSONStorage(() => localStorage),
      version: 1,
    },
  ),
);
