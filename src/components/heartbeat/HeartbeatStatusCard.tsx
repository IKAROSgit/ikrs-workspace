/**
 * Tier I + Tier II heartbeat status card (Phase E.8).
 *
 * Slots into the Settings page. Shows:
 *   - Tier I cadence (Rust loop tick count + last-fired timestamp)
 *   - Tier II health (latest heartbeat_health doc verdict)
 *   - "Run now" button (forces an immediate Tier I tick)
 *
 * Per spec §Tier I — verifying Tier II's writes is the operator-visible
 * surface for the heartbeat. Errors / staleness surface here so the
 * operator notices before the consultant calls them.
 */

import { useState, type ReactElement } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useEngagementStore } from "@/stores/engagementStore";
import { useAuth } from "@/providers/AuthProvider";
import { useHeartbeatTierI, type TierIVerdict } from "@/hooks/useHeartbeatTierI";

const VERDICT_LABEL: Record<TierIVerdict, string> = {
  healthy: "Healthy",
  stale: "Tier II stale",
  error: "Tier II error",
  unknown: "No data yet",
};

const VERDICT_COLOR: Record<TierIVerdict, "default" | "secondary" | "destructive"> = {
  healthy: "default",
  stale: "secondary",
  error: "destructive",
  unknown: "secondary",
};

const VERDICT_HELP: Record<TierIVerdict, string> = {
  healthy: "Tier II ran on schedule and reported a clean tick.",
  stale:
    "Tier II hasn't reported in over 2 hours. Check the VM's systemd timer (`systemctl --user status ikrs-heartbeat.timer`).",
  error:
    "Tier II's most recent tick reported an error. Check the audit log on the VM (~/_memory/heartbeat-log.jsonl) for details.",
  unknown:
    "No telemetry yet. Either Tier II hasn't run since this engagement was created, or the Firebase project isn't configured.",
};

function formatRelative(iso: string | null): string {
  if (!iso) return "never";
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return iso;
  const ageMs = Date.now() - ms;
  if (ageMs < 60_000) return "just now";
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)} min ago`;
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)} h ago`;
  return `${Math.floor(ageMs / 86_400_000)} d ago`;
}

export function HeartbeatStatusCard(): ReactElement | null {
  const { consultant } = useAuth();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const tenantId = consultant?.id ?? null;

  const { status, runNow } = useHeartbeatTierI(tenantId, activeEngagementId);
  const [running, setRunning] = useState(false);

  if (!activeEngagementId || !consultant) {
    return null;
  }

  const handleRunNow = async () => {
    setRunning(true);
    try {
      await runNow();
    } finally {
      // Re-enable after a short delay so the user can see feedback.
      setTimeout(() => setRunning(false), 1500);
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Heartbeat (Phase E)</CardTitle>
          <Badge variant={VERDICT_COLOR[status.verdict]}>
            {VERDICT_LABEL[status.verdict]}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <p className="text-muted-foreground">{VERDICT_HELP[status.verdict]}</p>

        <div className="grid grid-cols-2 gap-x-4 gap-y-1">
          <div className="text-muted-foreground">Tier II last tick</div>
          <div>{formatRelative(status.lastTickTs)}</div>

          <div className="text-muted-foreground">Tier II status</div>
          <div>{status.lastTickStatus ?? "(no data)"}</div>

          {status.lastErrorCode && (
            <>
              <div className="text-muted-foreground">Last error code</div>
              <div className="font-mono text-xs">{status.lastErrorCode}</div>
            </>
          )}

          <div className="text-muted-foreground">Tier I last fired</div>
          <div>
            {formatRelative(status.lastTierIRunAt)}
            {status.lastTrigger ? ` (${status.lastTrigger})` : ""}
          </div>

          <div className="text-muted-foreground">Tier I tick count</div>
          <div>{status.tickCount}</div>
        </div>

        <div className="flex items-center justify-between pt-2">
          <p className="text-xs text-muted-foreground">
            Tier I runs hourly while the app is open. Tier II runs hourly on
            the VM (24/7).
          </p>
          <Button
            size="sm"
            variant="outline"
            onClick={handleRunNow}
            disabled={running}
          >
            {running ? "Firing…" : "Run now"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
