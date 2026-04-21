import { type ReactNode, useCallback } from "react";
import {
  Panel,
  PanelGroup,
  PanelResizeHandle,
} from "react-resizable-panels";
import {
  useDeskspaceStore,
  type PanelViewKey,
} from "@/stores/deskspaceStore";

/**
 * Two-pane resizable wrapper used by views that have an optional
 * right-side contextual panel (ChatView, NotesView, CalendarView,
 * TasksView drawer).
 *
 * Design (completes Moe's "adaptive, dynamic, resizable" ask —
 * 2026-04-21):
 *  - Horizontal split; left = primary, right = optional.
 *  - Widths persisted per view in `useDeskspaceStore`.
 *  - Right rail is rendered only when the caller passes a
 *    non-null `right` node — otherwise the left spans 100% with
 *    no dangling handle.
 *  - Minimum sizes prevent slivers (20% each when both visible).
 *  - Uses `react-resizable-panels` (already a dependency) which
 *    renders pointer-handled drag, keyboard-accessible handles,
 *    and smooth layout animation on mount.
 *
 * Usage:
 *   <ResizableLayout viewKey="chat" right={<SavedFilesPanel />}>
 *     <Messages />
 *   </ResizableLayout>
 *
 * Intentionally a no-op when `right` is falsy so existing
 * conditional-panel logic in callers still works unchanged.
 */
export function ResizableLayout({
  viewKey,
  children,
  right,
}: {
  viewKey: PanelViewKey;
  children: ReactNode;
  right?: ReactNode | null;
}) {
  const sizes = useDeskspaceStore((s) => s.panelSizes[viewKey]);
  const setSizes = useDeskspaceStore((s) => s.setPanelSizes);

  const onLayout = useCallback(
    (next: number[]) => {
      if (next.length !== 2) return;
      const left = Math.round(next[0]!);
      const right = Math.round(next[1]!);
      // Avoid spamming localStorage — only persist changes >= 1pt.
      const [cl, cr] = sizes;
      if (Math.abs(cl - left) < 1 && Math.abs(cr - right) < 1) return;
      setSizes(viewKey, [left, right]);
    },
    [sizes, setSizes, viewKey],
  );

  if (!right) {
    return <div className="flex-1 flex overflow-hidden">{children}</div>;
  }

  return (
    <PanelGroup
      direction="horizontal"
      className="flex-1 overflow-hidden"
      autoSaveId={`ikrs-deskspace-${viewKey}`}
      onLayout={onLayout}
    >
      <Panel defaultSize={sizes[0]} minSize={20} className="overflow-hidden">
        <div className="h-full overflow-hidden flex flex-col">{children}</div>
      </Panel>
      <PanelResizeHandle className="w-1 bg-border hover:bg-primary/40 transition-colors cursor-col-resize" />
      <Panel defaultSize={sizes[1]} minSize={20} className="overflow-hidden">
        <div className="h-full overflow-hidden">{right}</div>
      </Panel>
    </PanelGroup>
  );
}
