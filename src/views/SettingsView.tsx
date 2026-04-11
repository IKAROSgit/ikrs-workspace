import { useState, useMemo } from "react";
import { homeDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { useAuth } from "@/providers/AuthProvider";
import { useEngagementActions } from "@/providers/EngagementProvider";
import { useEngagementStore } from "@/stores/engagementStore";
import { startOAuth, scaffoldEngagementSkills } from "@/lib/tauri-commands";
import { SkillStatusPanel } from "@/components/skills/SkillStatusPanel";
import type { SkillUpdateParams } from "@/types/skills";

const GOOGLE_SCOPES = [
  "https://www.googleapis.com/auth/gmail.modify",
  "https://www.googleapis.com/auth/calendar.events",
  "https://www.googleapis.com/auth/drive.file",
];

const OAUTH_CLIENT_ID = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID ?? "";
const OAUTH_PORT = 49152;

export default function SettingsView() {
  const { consultant, logOut } = useAuth();
  const { createClient, createEngagement } = useEngagementActions();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);

  const [clientName, setClientName] = useState("");
  const [clientDomain, setClientDomain] = useState("");
  const [engagementTitle, setEngagementTitle] = useState("");
  const [engagementDesc, setEngagementDesc] = useState("");
  const [creating, setCreating] = useState(false);
  const [oauthStatus, setOauthStatus] = useState<
    "idle" | "pending" | "success" | "error"
  >("idle");

  const skillUpdateParams: SkillUpdateParams | null = useMemo(() => {
    if (!activeEngagementId || !consultant) return null;
    const engagement = engagements.find((e) => e.id === activeEngagementId);
    if (!engagement) return null;
    const client = clients.find((c) => c.id === engagement.clientId);
    if (!client) return null;

    return {
      engagementPath: engagement.vault.path,
      clientName: client.name,
      clientSlug: client.slug,
      engagementTitle: engagement.settings.description ?? `${client.name} Engagement`,
      engagementDescription: engagement.settings.description ?? `Engagement for ${client.name}`,
      consultantName: consultant.name,
      consultantEmail: consultant.email,
      timezone: engagement.settings.timezone,
      startDate: engagement.startDate instanceof Date
        ? engagement.startDate.toISOString().split("T")[0]!
        : String(engagement.startDate).split("T")[0]!,
    };
  }, [activeEngagementId, consultant, engagements, clients]);

  const handleCreateEngagement = async () => {
    if (!clientName || !clientDomain || !consultant) return;
    setCreating(true);

    try {
      const slug = clientDomain.replace(/\./g, "-").toLowerCase();
      const home = await homeDir();
      const vaultPath = `${home}.ikrs-workspace/vaults/${slug}/`;

      const clientId = await createClient({
        name: clientName,
        domain: clientDomain,
        slug,
        branding: {},
      });

      const engId = await createEngagement({
        consultantId: consultant.id,
        clientId,
        status: "active",
        startDate: new Date(),
        settings: {
          timezone: consultant.preferences.timezone,
          description: engagementDesc || undefined,
        },
        vault: {
          path: vaultPath,
          status: "active",
        },
      });

      // Scaffold skill folders on disk (Phase 2)
      await scaffoldEngagementSkills({
        engagementPath: vaultPath,
        clientName,
        clientSlug: slug,
        engagementTitle: engagementTitle || `${clientName} Engagement`,
        engagementDescription: engagementDesc || `Engagement for ${clientName}`,
        consultantName: consultant.name,
        consultantEmail: consultant.email,
        timezone: consultant.preferences.timezone,
      });

      useEngagementStore.getState().setActiveEngagement(engId);
      setClientName("");
      setClientDomain("");
      setEngagementTitle("");
      setEngagementDesc("");
    } catch (err) {
      console.error("Failed to create engagement:", err);
    } finally {
      setCreating(false);
    }
  };

  const handleConnectGoogle = async () => {
    if (!activeEngagementId) return;
    setOauthStatus("pending");
    try {
      const { auth_url } = await startOAuth(
        OAUTH_CLIENT_ID,
        OAUTH_PORT,
        GOOGLE_SCOPES,
      );
      await open(auth_url);
      setOauthStatus("success");
    } catch {
      setOauthStatus("error");
    }
  };

  return (
    <div className="flex flex-col gap-6 p-6 max-w-2xl">
      <Card>
        <CardHeader>
          <CardTitle>Profile</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          <p>
            <strong>Name:</strong> {consultant?.name}
          </p>
          <p>
            <strong>Email:</strong> {consultant?.email}
          </p>
          <p>
            <strong>Role:</strong>{" "}
            <Badge variant="secondary">{consultant?.role}</Badge>
          </p>
          <Separator />
          <Button variant="destructive" size="sm" onClick={logOut}>
            Sign out
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>New Engagement</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input
            placeholder="Client name (e.g. BLR WORLD)"
            value={clientName}
            onChange={(e) => setClientName(e.target.value)}
          />
          <Input
            placeholder="Client domain (e.g. blr-world.com)"
            value={clientDomain}
            onChange={(e) => setClientDomain(e.target.value)}
          />
          <Input
            placeholder="Engagement title (e.g. Annual Gala 2026)"
            value={engagementTitle}
            onChange={(e) => setEngagementTitle(e.target.value)}
          />
          <Input
            placeholder="Description (optional)"
            value={engagementDesc}
            onChange={(e) => setEngagementDesc(e.target.value)}
          />
          <Button
            onClick={handleCreateEngagement}
            disabled={!clientName || !clientDomain || creating}
          >
            {creating ? "Creating..." : "Create engagement"}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Google Account</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {!activeEngagementId ? (
            <p className="text-muted-foreground">
              Select an engagement first.
            </p>
          ) : (
            <>
              <Button
                onClick={handleConnectGoogle}
                disabled={oauthStatus === "pending"}
              >
                {oauthStatus === "pending"
                  ? "Connecting..."
                  : "Connect Google Account"}
              </Button>
              {oauthStatus === "success" && (
                <p className="text-green-500 text-sm">
                  Connected successfully.
                </p>
              )}
              {oauthStatus === "error" && (
                <p className="text-red-500 text-sm">
                  Connection failed. Try again.
                </p>
              )}
            </>
          )}
        </CardContent>
      </Card>

      {activeEngagementId && (
        <SkillStatusPanel updateParams={skillUpdateParams} />
      )}
    </div>
  );
}
