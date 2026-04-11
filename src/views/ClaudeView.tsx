import { useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Bot, Play, AlertTriangle, CheckCircle } from "lucide-react";
import { useClaude } from "@/hooks/useClaude";
import { useEngagementStore } from "@/stores/engagementStore";

export default function ClaudeView() {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const { session, isInstalled, launching, error, checkInstalled, launch } = useClaude();

  useEffect(() => {
    checkInstalled();
  }, [checkInstalled]);

  if (!activeEngagementId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Bot size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to use Claude Code.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-6 max-w-lg">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Bot size={20} />
            Claude Code
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {isInstalled === false && (
            <div className="flex items-center gap-2 text-yellow-500 text-sm">
              <AlertTriangle size={16} />
              Claude CLI not found. Install it first.
            </div>
          )}

          {isInstalled && !session && (
            <Button onClick={launch} disabled={launching} className="w-full">
              <Play size={16} className="mr-2" />
              {launching ? "Launching..." : "Open Claude Code"}
            </Button>
          )}

          {session && (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <CheckCircle size={16} className="text-green-500" />
                <Badge variant="secondary">Running</Badge>
              </div>
              <p className="text-xs text-muted-foreground">PID: {session.pid}</p>
              <p className="text-xs text-muted-foreground">
                Started: {session.startedAt.toLocaleTimeString()}
              </p>
              <p className="text-xs text-muted-foreground truncate">
                Project: {session.projectPath}
              </p>
            </div>
          )}

          {error && (
            <p className="text-sm text-destructive">{error}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
