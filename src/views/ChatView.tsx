import { useEffect, useRef, useCallback } from "react";
import { Bot } from "lucide-react";
import { Button } from "@/components/ui/button";
import { MessageBubble } from "@/components/chat/MessageBubble";
import { ToolActivityCard } from "@/components/chat/ToolActivityCard";
import { InputBar } from "@/components/chat/InputBar";
import { SessionIndicator } from "@/components/chat/SessionIndicator";
import { useClaudeStream } from "@/hooks/useClaudeStream";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import {
  claudeAuthStatus,
  claudeVersionCheck,
  spawnClaudeSession,
  sendClaudeMessage,
} from "@/lib/tauri-commands";

export default function ChatView() {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId)
  );

  const sessionId = useClaudeStore((s) => s.sessionId);
  const status = useClaudeStore((s) => s.status);
  const messages = useClaudeStore((s) => s.messages);
  const activeTools = useClaudeStore((s) => s.activeTools);
  const totalCostUsd = useClaudeStore((s) => s.totalCostUsd);
  const model = useClaudeStore((s) => s.model);
  const error = useClaudeStore((s) => s.error);

  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Subscribe to Tauri events
  useClaudeStream();

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, activeTools]);

  const handleConnect = useCallback(async () => {
    if (!engagement) return;

    // Preflight checks
    const version = await claudeVersionCheck();
    if (!version.installed) {
      useClaudeStore.getState().setError(
        "Claude CLI not found. Please install Claude Code first."
      );
      return;
    }
    if (!version.meets_minimum) {
      useClaudeStore.getState().setError(
        `Claude CLI ${version.version} is too old. Please update to ${version.version} or later.`
      );
      return;
    }

    const auth = await claudeAuthStatus();
    if (!auth.loggedIn) {
      useClaudeStore.getState().setError(
        "Not signed in to Claude. Please sign in first from Settings."
      );
      return;
    }

    useClaudeStore.getState().reset();
    useClaudeStore.setState({ status: "connecting" });

    try {
      await spawnClaudeSession(engagement.id, engagement.vault.path);
    } catch (e) {
      useClaudeStore.getState().setError(
        e instanceof Error ? e.message : String(e)
      );
    }
  }, [engagement]);

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
      <SessionIndicator status={status} model={model} costUsd={totalCostUsd} />

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

        <div ref={messagesEndRef} />
      </div>

      <InputBar
        onSend={handleSend}
        disabled={status === "thinking" || status === "disconnected" || status === "connecting"}
        placeholder={
          status === "thinking"
            ? "Claude is thinking..."
            : "Ask Claude anything..."
        }
      />
    </div>
  );
}
