import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ChevronDown, Plus } from "lucide-react";
import { useEngagementStore } from "@/stores/engagementStore";

interface EngagementSwitcherProps {
  onCreateNew: () => void;
}

export function EngagementSwitcher({ onCreateNew }: EngagementSwitcherProps) {
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const setActiveEngagement = useEngagementStore((s) => s.setActiveEngagement);

  const activeEngagement = engagements.find((e) => e.id === activeEngagementId);
  const activeClient = clients.find((c) => c.id === activeEngagement?.clientId);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="inline-flex w-full items-center justify-between gap-1 rounded-lg border border-transparent bg-clip-padding px-2 text-sm font-medium whitespace-nowrap transition-all h-10 hover:bg-muted hover:text-foreground"
      >
        <span className="truncate">
          {activeClient?.name ?? "Select engagement"}
        </span>
        <ChevronDown size={14} className="shrink-0" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {engagements
          .filter((e) => e.status === "active")
          .map((eng) => {
            const client = clients.find((c) => c.id === eng.clientId);
            return (
              <DropdownMenuItem
                key={eng.id}
                onClick={() => setActiveEngagement(eng.id)}
                className={eng.id === activeEngagementId ? "bg-accent" : ""}
              >
                {client?.name ?? "Unknown client"}
              </DropdownMenuItem>
            );
          })}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={onCreateNew}>
          <Plus size={14} className="mr-2" />
          New engagement
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
