import { useEffect, useRef, useCallback, useState } from "react";
import { Bot } from "lucide-react";
import { Button } from "@/components/ui/button";
import { MessageBubble } from "@/components/chat/MessageBubble";
import { ToolActivityCard } from "@/components/chat/ToolActivityCard";
import { InputBar } from "@/components/chat/InputBar";
import { SessionIndicator } from "@/components/chat/SessionIndicator";
import { SavedFilesPanel } from "@/components/chat/SavedFilesPanel";
import { ResizableLayout } from "@/components/layout/ResizableLayout";
import { useClaudeStream } from "@/hooks/useClaudeStream";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import { useWorkspaceSession } from "@/hooks/useWorkspaceSession";
import { useOnlineStatus } from "@/hooks/useOnlineStatus";
import { OfflineBanner } from "@/components/OfflineBanner";
import { sendClaudeMessage, startOAuthFlow, cancelOAuthFlow, killClaudeSession, getCredential, makeKeychainKey } from "@/lib/tauri-commands";
import { syncTokenToFirestore } from "@/lib/firestore-tokens";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

// Google OAuth scopes. 2026-04-20: drive.file → drive.readonly so
// the Files view surfaces the consultant's existing Drive content
// (drive.file only exposes files the app itself created, which would
// make the view look empty on day 0). Still read-only; Claude's
// in-chat tools that need write access use the drive MCP which has
// its own auth.
const GOOGLE_SCOPES = [
  "https://www.googleapis.com/auth/gmail.modify",
  "https://www.googleapis.com/auth/calendar.events",
  "https://www.googleapis.com/auth/drive.readonly",
];
const OAUTH_CLIENT_ID = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID ?? "";
const OAUTH_CLIENT_SECRET = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_SECRET ?? "";
// 2026-04-20: moved off 49152 because macOS's rapportd daemon binds
// that port on IPv6 wildcard, and modern macOS prefers IPv6 for
// `localhost` resolution — browser callbacks hit rapportd not us.
// 53111 is in the IANA private range and we've verified it's clear.
const OAUTH_PORT = 53111;

export default function ChatView() {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  const sessionId = useClaudeStore((s) => s.sessionId);
  const status = useClaudeStore((s) => s.status);
  const messages = useClaudeStore((s) => s.messages);
  const activeTools = useClaudeStore((s) => s.activeTools);
  const totalCostUsd = useClaudeStore((s) => s.totalCostUsd);
  const model = useClaudeStore((s) => s.model);
  const error = useClaudeStore((s) => s.error);
  const writeVerifications = useClaudeStore((s) => s.writeVerifications);

  const authError = useClaudeStore((s) => s.authError);
  const clearAuthError = useClaudeStore((s) => s.clearAuthError);

  const { connect: handleConnect, switching } = useWorkspaceSession();
  const isOnline = useOnlineStatus();

  const [reauthing, setReauthing] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Subscribe to Tauri events
  useClaudeStream();

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, activeTools]);

  const handleSend = useCallback(
    async (text: string) => {
      if (!sessionId) return;
      useClaudeStore.getState().addUserMessage(text);
      try {
        await sendClaudeMessage(sessionId, text);
      } catch (e) {
        useClaudeStore.getState().setError(
          e instanceof Error ? e.message : String(e)
        );
      }
    },
    [sessionId]
  );

  const handleReauth = useCallback(async () => {
    if (reauthing) return;
    setReauthing(true);
    clearAuthError();
    try {
      const unlisten = await listen<{ keychain_key: string }>(
        "oauth:token-stored",
        async () => {
          unlisten();
          // Phase F: sync encrypted token to Firestore for heartbeat
          if (activeEngagementId) {
            try {
              const keychainKey = makeKeychainKey(activeEngagementId, "google");
              const payload = await getCredential(keychainKey);
              if (payload) {
                await syncTokenToFirestore(activeEngagementId, payload);
              }
            } catch (e) {
              const msg = e instanceof Error ? e.message : String(e);
              console.error("[Phase F] Firestore token sync failed:", msg);
              alert(
                "Google connected, but Firestore sync failed:\n\n" +
                msg + "\n\n" +
                "The heartbeat will NOT see this token until fixed.",
              );
            }
          }
          const sid = useClaudeStore.getState().sessionId;
          if (sid) await killClaudeSession(sid);
          await handleConnect();
          setReauthing(false);
        }
      );

      const { auth_url } = await startOAuthFlow(
        activeEngagementId!,
        OAUTH_CLIENT_ID,
        OAUTH_CLIENT_SECRET,
        OAUTH_PORT,
        GOOGLE_SCOPES
      );
      await openUrl(auth_url);

      setTimeout(async () => {
        unlisten();
        await cancelOAuthFlow();
        setReauthing(false);
      }, 5 * 60 * 1000);
    } catch (e) {
      useClaudeStore.getState().setError(
        `Re-auth failed: ${e instanceof Error ? e.message : String(e)}`
      );
      setReauthing(false);
    }
  }, [reauthing, clearAuthError, handleConnect, activeEngagementId]);

  // No engagement selected
  if (!activeEngagementId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Bot size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to use Claude.</p>
      </div>
    );
  }

  // Not connected yet
  if (status === "disconnected" && !error) {
    return (
      <div className="flex flex-col h-full">
        <OfflineBanner feature="Claude" />
        <div className="flex flex-col items-center justify-center flex-1 gap-4">
          <Bot size={48} className="text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            Start a Claude session for this engagement
          </p>
          <Button onClick={handleConnect} disabled={!isOnline} title={!isOnline ? "Requires internet connection." : undefined}>Connect to Claude</Button>
        </div>
      </div>
    );
  }

  // Session dropped mid-use (webview refresh, CLI crash, stdin EOF,
  // etc). We have prior messages in the store but no active session
  // to send the next turn to. Show a prominent reconnect banner
  // rather than silently showing the disconnected state — users
  // reported losing the "I can restart this" affordance when the
  // webview reloads on Cmd+R. 2026-04-21 fix.
  const sessionDropped =
    (status === "disconnected" || status === "error") &&
    messages.length > 0 &&
    isOnline &&
    !switching;

  return (
    <div className="flex flex-col h-full">
      <OfflineBanner feature="Claude" />
      <SessionIndicator status={status} model={model} costUsd={totalCostUsd} switching={switching} />
      {sessionDropped && (
        <div className="flex items-center gap-3 px-4 py-2 bg-amber-500/15 border-b border-amber-500/30 text-amber-600 dark:text-amber-400 text-sm">
          <Bot size={14} className="flex-shrink-0" />
          <span className="flex-1">
            {status === "error"
              ? "Claude session stopped. Your chat history is saved — reconnect to continue."
              : "Claude session disconnected. Reconnect to keep working."}
          </span>
          <Button size="sm" onClick={handleConnect}>
            Reconnect
          </Button>
        </div>
      )}

      {/* Body: messages + saved-files ledger in a resizable 2-pane. */}
      <ResizableLayout
        viewKey="chat"
        right={writeVerifications.length > 0 ? <SavedFilesPanel /> : null}
      >
        <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {/* Tool activity cards (shown between messages) */}
        {activeTools
          .filter((t) => t.status === "running")
          .map((tool) => (
            <ToolActivityCard key={tool.toolId} tool={tool} />
          ))}

        {error && (
          <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive text-sm">
            <span>
              {!isOnline
                ? "Connection interrupted. Your work is saved locally."
                : error}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={handleConnect}
              className="ml-auto"
            >
              Retry
            </Button>
          </div>
        )}

        {authError && (
          <div className="flex items-center gap-2 p-3 rounded-md bg-amber-500/10 text-amber-700 dark:text-amber-400 text-sm">
            <span>
              Google authentication expired for {authError.server}. Re-authenticate to restore access.
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={handleReauth}
              disabled={reauthing}
              className="ml-auto"
            >
              {reauthing ? "Waiting for sign-in..." : "Re-authenticate"}
            </Button>
          </div>
        )}

        <div ref={messagesEndRef} />
        </div>
      </ResizableLayout>

      <InputBar
        onSend={handleSend}
        disabled={status === "thinking" || status === "disconnected" || status === "connecting" || switching}
        placeholder={
          status === "thinking"
            ? "Claude is thinking..."
            : "Ask Claude anything..."
        }
      />
    </div>
  );
}
