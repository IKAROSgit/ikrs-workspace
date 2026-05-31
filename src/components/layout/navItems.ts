import { Mail, Calendar, FolderOpen, CheckSquare, FileText, Bot, Settings } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ViewId } from "@/Router";

export interface NavItem {
  id: ViewId;
  icon: LucideIcon;
  label: string;
}

/**
 * Shared navigation entries consumed by both the desktop `SideRail`
 * (icon-only) and the mobile `MobileSidebar` (icon + label in a drawer).
 * Adding or reordering nav lives here — both rails update automatically.
 */
export const NAV_ITEMS: NavItem[] = [
  { id: "inbox", icon: Mail, label: "Inbox" },
  { id: "calendar", icon: Calendar, label: "Calendar" },
  { id: "files", icon: FolderOpen, label: "Files" },
  { id: "tasks", icon: CheckSquare, label: "Tasks" },
  { id: "notes", icon: FileText, label: "Notes" },
  { id: "claude", icon: Bot, label: "Claude Code" },
  { id: "settings", icon: Settings, label: "Settings" },
];
