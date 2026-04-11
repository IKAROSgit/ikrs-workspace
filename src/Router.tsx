import React, { lazy, Suspense } from "react";

const InboxView = lazy(() => import("@/views/InboxView"));
const CalendarView = lazy(() => import("@/views/CalendarView"));
const FilesView = lazy(() => import("@/views/FilesView"));
const TasksView = lazy(() => import("@/views/TasksView"));
const NotesView = lazy(() => import("@/views/NotesView"));
const ClaudeView = lazy(() => import("@/views/ClaudeView"));
const SettingsView = lazy(() => import("@/views/SettingsView"));

export type ViewId = "inbox" | "calendar" | "files" | "tasks" | "notes" | "claude" | "settings";

const VIEW_MAP: Record<ViewId, React.LazyExoticComponent<() => React.JSX.Element>> = {
  inbox: InboxView,
  calendar: CalendarView,
  files: FilesView,
  tasks: TasksView,
  notes: NotesView,
  claude: ClaudeView,
  settings: SettingsView,
};

export function ViewRouter({ activeView }: { activeView: ViewId }) {
  const ViewComponent = VIEW_MAP[activeView];
  return (
    <Suspense fallback={<div className="flex items-center justify-center h-full text-muted-foreground">Loading...</div>}>
      <ViewComponent />
    </Suspense>
  );
}
