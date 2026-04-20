import { useState, useMemo, useEffect } from "react";
import { homeDir, join } from "@tauri-apps/api/path";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { useAuth } from "@/providers/AuthProvider";
import { useEngagementActions } from "@/providers/EngagementProvider";
import { useEngagementStore } from "@/stores/engagementStore";
import {
  cancelOAuthFlow,
  getCredential,
  makeKeychainKey,
  scaffoldEngagementSkills,
  startOAuthFlow,
} from "@/lib/tauri-commands";
import { SkillStatusPanel } from "@/components/skills/SkillStatusPanel";
import { UpdateChecker } from "@/components/UpdateChecker";
import { useOnlineStatus } from "@/hooks/useOnlineStatus";
import type { SkillUpdateParams } from "@/types/skills";

// Keep in sync with ChatView.tsx GOOGLE_SCOPES. 2026-04-20:
// drive.file bumped to drive.readonly so the Files view surfaces
// consultants' existing Drive content.
const GOOGLE_SCOPES = [
  "https://www.googleapis.com/auth/gmail.modify",
  "https://www.googleapis.com/auth/calendar.events",
  "https://www.googleapis.com/auth/drive.readonly",
];

const OAUTH_CLIENT_ID = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID ?? "";
const OAUTH_CLIENT_SECRET = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_SECRET ?? "";
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
  const isOnline = useOnlineStatus();
  const [oauthStatus, setOauthStatus] = useState<
    "idle" | "pending" | "success" | "error"
  >("idle");

  // Hydrate OAuth connection state from the OS keychain whenever the
  // active engagement changes. Prior behaviour: `oauthStatus` was
  // component-local, so navigating away from Settings and back reset
  // the "Connected successfully" indicator to "idle" even though the
  // token remained in the keychain. Moe reported this as
  // "as soon as I change tabs I guess it's gone" 2026-04-18.
  useEffect(() => {
    if (!activeEngagementId) {
      setOauthStatus("idle");
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const key = makeKeychainKey(activeEngagementId, "google");
        const value = await getCredential(key);
        if (!cancelled) {
          setOauthStatus(value ? "success" : "idle");
        }
      } catch {
        if (!cancelled) setOauthStatus("idle");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeEngagementId]);

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
        ? engagement.startDate.toISOString().split("T")[0] ?? ""
        : String(engagement.startDate).split("T")[0] ?? "",
    };
  }, [activeEngagementId, consultant, engagements, clients]);

  const handleCreateEngagement = async () => {
    if (!clientName || !clientDomain || !consultant) return;
    setCreating(true);

    try {
      const slug = clientDomain.replace(/\./g, "-").toLowerCase();
      const home = await homeDir();
      // Codex S6 (2026-04-17): `${home}.ikrs-workspace/…` produces
      // `/Users/<user>.ikrs-workspace/…` on macOS because `homeDir()`
      // returns without a trailing slash. Use path.join to normalise
      // platform-correctly. Trailing slash re-appended because downstream
      // Rust (validate_engagement_path, vault scaffolders) expects the
      // vault path to end with `/`.
      const joined = await join(home, ".ikrs-workspace", "vaults", slug);
      const vaultPath = joined.endsWith("/") ? joined : `${joined}/`;

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

    let unlisten: (() => void) | undefined;
    let timeout: ReturnType<typeof setTimeout> | undefined;

    try {
      // Subscribe to token-stored event BEFORE starting the flow
      const tokenPromise = new Promise<boolean>((resolve) => {
        listen("oauth:token-stored", () => {
          resolve(true);
        }).then((fn) => {
          unlisten = fn;
        });

        timeout = setTimeout(() => {
          resolve(false);
        }, 300_000); // 5-minute timeout
      });

      const { auth_url } = await startOAuthFlow(
        activeEngagementId,
        OAUTH_CLIENT_ID,
        OAUTH_CLIENT_SECRET,
        OAUTH_PORT,
        GOOGLE_SCOPES,
      );
      await openUrl(auth_url);

      const success = await tokenPromise;
      setOauthStatus(success ? "success" : "error");

      if (!success) {
        await cancelOAuthFlow();
      }
    } catch {
      setOauthStatus("error");
    } finally {
      unlisten?.();
      if (timeout) clearTimeout(timeout);
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
                disabled={oauthStatus === "pending" || !isOnline}
                title={!isOnline ? "Sign in requires internet." : undefined}
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

      <Card>
        <CardHeader>
          <CardTitle>About</CardTitle>
        </CardHeader>
        <CardContent>
          <UpdateChecker />
        </CardContent>
      </Card>
    </div>
  );
}
