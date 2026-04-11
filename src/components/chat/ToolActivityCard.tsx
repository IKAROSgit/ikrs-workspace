import { cn } from "@/lib/utils";
import { Loader2, CheckCircle, XCircle } from "lucide-react";
import type { ToolActivity } from "@/types/claude";

interface ToolActivityCardProps {
  tool: ToolActivity;
}

const TOOL_ICONS: Record<string, string> = {
  Write: "\u{1F4DD}",
  Edit: "\u{270F}\u{FE0F}",
  Read: "\u{1F4D6}",
  Glob: "\u{1F50D}",
  Grep: "\u{1F50D}",
  WebSearch: "\u{1F310}",
  WebFetch: "\u{1F310}",
};

export function ToolActivityCard({ tool }: ToolActivityCardProps) {
  const icon = TOOL_ICONS[tool.toolName] ?? "\u{2699}\u{FE0F}";

  return (
    <div
      className={cn(
        "flex items-center gap-2 px-3 py-1.5 rounded-md text-xs",
        "bg-muted/50 border border-border/50"
      )}
    >
      <span>{icon}</span>
      <span className="flex-1 truncate">{tool.friendlyLabel}</span>
      {tool.status === "running" && (
        <Loader2 size={12} className="animate-spin text-muted-foreground" />
      )}
      {tool.status === "success" && (
        <CheckCircle size={12} className="text-green-500" />
      )}
      {tool.status === "error" && (
        <XCircle size={12} className="text-destructive" />
      )}
    </div>
  );
}
