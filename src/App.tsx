import { TooltipProvider } from "@/components/ui/tooltip";
import { AuthProvider, AuthGate } from "@/providers/AuthProvider";
import { EngagementProvider } from "@/providers/EngagementProvider";
import { SideRail } from "@/components/layout/SideRail";
import { EngagementSwitcher } from "@/components/layout/EngagementSwitcher";
import { Toolbar } from "@/components/layout/Toolbar";
import { StatusBar } from "@/components/layout/StatusBar";
import { ViewRouter } from "@/Router";
import { useUiStore } from "@/stores/uiStore";
import { useEngagementStore } from "@/stores/engagementStore";
import { useMcpStore } from "@/stores/mcpStore";
import { useOnlineStatus } from "@/hooks/useOnlineStatus";
import { CommandPalette } from "@/components/CommandPalette";

function AppContent() {
  const activeView = useUiStore((s) => s.activeView);
  const setActiveView = useUiStore((s) => s.setActiveView);
  const activeEngagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId)
  );
  const clients = useEngagementStore((s) => s.clients);
  const mcpServers = useMcpStore((s) => s.servers);
  const isOnline = useOnlineStatus();

  const activeClient = clients.find((c) => c.id === activeEngagement?.clientId);

  return (
    <div className="flex flex-col h-screen bg-background text-foreground">
      <div className="flex flex-1 overflow-hidden">
        <div className="flex flex-col w-14 bg-sidebar border-r border-border">
          <div className="p-1 border-b border-border">
            <EngagementSwitcher onCreateNew={() => setActiveView("settings")} />
          </div>
          <SideRail activeView={activeView} onNavigate={setActiveView} />
        </div>
        <div className="flex flex-col flex-1 overflow-hidden">
          <Toolbar engagementName={activeClient?.name ?? "No engagement selected"} />
          <main className="flex-1 overflow-auto">
            <ViewRouter activeView={activeView} />
          </main>
        </div>
      </div>
      <StatusBar connectedEmail={undefined} mcpStatuses={mcpServers} isOnline={isOnline} />
      <CommandPalette />
    </div>
  );
}

export default function App() {
  return (
    <AuthProvider>
      <TooltipProvider>
        <AuthGate>
          <EngagementProvider>
            <AppContent />
          </EngagementProvider>
        </AuthGate>
      </TooltipProvider>
    </AuthProvider>
  );
}
