import { useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { useAuth } from "@/providers/AuthProvider";
import { useEngagementActions } from "@/providers/EngagementProvider";
import { useEngagementStore } from "@/stores/engagementStore";
import { startOAuth } from "@/lib/tauri-commands";

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

  const [clientName, setClientName] = useState("");
  const [clientDomain, setClientDomain] = useState("");
  const [oauthStatus, setOauthStatus] = useState<
    "idle" | "pending" | "success" | "error"
  >("idle");

  const handleCreateEngagement = async () => {
    if (!clientName || !clientDomain || !consultant) return;
    const slug = clientDomain.replace(/\./g, "-").toLowerCase();
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
      settings: { timezone: consultant.preferences.timezone },
      vault: {
        path: `~/.ikrs-workspace/vaults/${slug}/`,
        status: "active",
      },
    });
    useEngagementStore.getState().setActiveEngagement(engId);
    setClientName("");
    setClientDomain("");
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
          <Button
            onClick={handleCreateEngagement}
            disabled={!clientName || !clientDomain}
          >
            Create engagement
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
    </div>
  );
}
