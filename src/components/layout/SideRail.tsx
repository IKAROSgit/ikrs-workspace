import { Mail, Calendar, FolderOpen, CheckSquare, FileText, Bot, Settings } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { ViewId } from "@/Router";

const NAV_ITEMS: { id: ViewId; icon: typeof Mail; label: string }[] = [
  { id: "inbox", icon: Mail, label: "Inbox" },
  { id: "calendar", icon: Calendar, label: "Calendar" },
  { id: "files", icon: FolderOpen, label: "Files" },
  { id: "tasks", icon: CheckSquare, label: "Tasks" },
  { id: "notes", icon: FileText, label: "Notes" },
  { id: "claude", icon: Bot, label: "Claude Code" },
  { id: "settings", icon: Settings, label: "Settings" },
];

interface SideRailProps {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
}

export function SideRail({ activeView, onNavigate }: SideRailProps) {
  return (
    <nav className="flex flex-col items-center w-14 bg-sidebar border-r border-border py-4 gap-2">
      {NAV_ITEMS.map(({ id, icon: Icon, label }) => (
        <Tooltip key={id}>
          <TooltipTrigger
            render={
              <button
                onClick={() => onNavigate(id)}
                className={`w-10 h-10 flex items-center justify-center rounded-lg transition-colors ${
                  activeView === id
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                }`}
                aria-label={label}
                aria-current={activeView === id ? "page" : undefined}
              >
                <Icon size={20} />
              </button>
            }
          />
          <TooltipContent side="right">{label}</TooltipContent>
        </Tooltip>
      ))}
    </nav>
  );
}
