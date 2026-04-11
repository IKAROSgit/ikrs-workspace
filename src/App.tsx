import { useState } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AuthProvider, AuthGate } from "@/providers/AuthProvider";
import { SideRail } from "@/components/layout/SideRail";
import { Toolbar } from "@/components/layout/Toolbar";
import { StatusBar } from "@/components/layout/StatusBar";
import { ViewRouter, type ViewId } from "@/Router";

export default function App() {
  const [activeView, setActiveView] = useState<ViewId>("inbox");

  return (
    <AuthProvider>
      <TooltipProvider>
        <div className="flex flex-col h-screen bg-background text-foreground">
          <AuthGate>
            <div className="flex flex-1 overflow-hidden">
              <SideRail activeView={activeView} onNavigate={setActiveView} />
              <div className="flex flex-col flex-1 overflow-hidden">
                <Toolbar engagementName="No engagement selected" />
                <main className="flex-1 overflow-auto">
                  <ViewRouter activeView={activeView} />
                </main>
              </div>
            </div>
            <StatusBar connectedEmail={undefined} mcpStatuses={[]} isOnline={true} />
          </AuthGate>
        </div>
      </TooltipProvider>
    </AuthProvider>
  );
}
