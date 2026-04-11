import { Search, Settings2 } from "lucide-react";
import { Input } from "@/components/ui/input";

interface ToolbarProps {
  engagementName: string;
}

export function Toolbar({ engagementName }: ToolbarProps) {
  return (
    <header className="flex items-center h-12 px-4 border-b border-border bg-background gap-4">
      <span className="font-semibold text-sm truncate">{engagementName}</span>
      <div className="flex-1 max-w-md">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" size={14} />
          <Input
            placeholder="Search... (Cmd+K)"
            className="h-8 pl-8 text-sm"
            readOnly
          />
        </div>
      </div>
      <button className="p-2 text-muted-foreground hover:text-foreground" aria-label="Settings">
        <Settings2 size={16} />
      </button>
    </header>
  );
}
