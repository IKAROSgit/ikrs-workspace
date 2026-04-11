import React, { lazy, Suspense } from "react";
import { ViewErrorBoundary } from "@/components/ViewErrorBoundary";

const InboxView = lazy(() => import("@/views/InboxView"));
const CalendarView = lazy(() => import("@/views/CalendarView"));
const FilesView = lazy(() => import("@/views/FilesView"));
const TasksView = lazy(() => import("@/views/TasksView"));
const NotesView = lazy(() => import("@/views/NotesView"));
const ClaudeView = lazy(() => import("@/views/ClaudeView"));
const SettingsView = lazy(() => import("@/views/SettingsView"));

export type ViewId = "inbox" | "calendar" | "files" | "tasks" | "notes" | "claude" | "settings";

const VIEW_MAP: Record<ViewId, { component: React.LazyExoticComponent<() => React.JSX.Element>; label: string }> = {
  inbox: { component: InboxView, label: "Inbox" },
  calendar: { component: CalendarView, label: "Calendar" },
  files: { component: FilesView, label: "Files" },
  tasks: { component: TasksView, label: "Tasks" },
  notes: { component: NotesView, label: "Notes" },
  claude: { component: ClaudeView, label: "Claude Code" },
  settings: { component: SettingsView, label: "Settings" },
};

export function ViewRouter({ activeView }: { activeView: ViewId }) {
  const { component: ViewComponent, label } = VIEW_MAP[activeView];
  return (
    <ViewErrorBoundary viewName={label} key={activeView}>
      <Suspense
        fallback={
          <div className="flex items-center justify-center h-full text-muted-foreground">
            Loading...
          </div>
        }
      >
        <ViewComponent />
      </Suspense>
    </ViewErrorBoundary>
  );
}
