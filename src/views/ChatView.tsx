import { useEffect, useRef, useCallback, useState } from "react";
import { Bot } from "lucide-react";
import { Button } from "@/components/ui/button";
import { MessageBubble } from "@/components/chat/MessageBubble";
import { ToolActivityCard } from "@/components/chat/ToolActivityCard";
import { InputBar } from "@/components/chat/InputBar";
import { SessionIndicator } from "@/components/chat/SessionIndicator";
import { useClaudeStream } from "@/hooks/useClaudeStream";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import { useWorkspaceSession } from "@/hooks/useWorkspaceSession";
import { sendClaudeMessage, startOAuthFlow, cancelOAuthFlow, killClaudeSession } from "@/lib/tauri-commands";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-opener";

const GOOGLE_SCOPES = [
  "https://www.googleapis.com/auth/gmail.modify",
  "https://www.googleapis.com/auth/calendar.events",
  "https://www.googleapis.com/auth/drive.file",
];
const OAUTH_CLIENT_ID = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID ?? "";
const OAUTH_PORT = 49152;

export default function ChatView() {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  const sessionId = useClaudeStore((s) => s.sessionId);
  const status = useClaudeStore((s) => s.status);
  const messages = useClaudeStore((s) => s.messages);
  const activeTools = useClaudeStore((s) => s.activeTools);
  const totalCostUsd = useClaudeStore((s) => s.totalCostUsd);
  const model = useClaudeStore((s) => s.model);
  const error = useClaudeStore((s) => s.error);

  const authError = useClaudeStore((s) => s.authError);
  const clearAuthError = useClaudeStore((s) => s.clearAuthError);

  const { connect: handleConnect, switching } = useWorkspaceSession();

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
          const sid = useClaudeStore.getState().sessionId;
          if (sid) await killClaudeSession(sid);
          await handleConnect();
          setReauthing(false);
        }
      );

      const { auth_url } = await startOAuthFlow(
        activeEngagementId!,
        OAUTH_CLIENT_ID,
        OAUTH_PORT,
        GOOGLE_SCOPES
      );
      await open(auth_url);

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
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <Bot size={48} className="text-muted-foreground" />
        <p className="text-sm text-muted-foreground">
          Start a Claude session for this engagement
        </p>
        <Button onClick={handleConnect}>Connect to Claude</Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <SessionIndicator status={status} model={model} costUsd={totalCostUsd} switching={switching} />

      {/* Messages area */}
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
            <span>{error}</span>
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
