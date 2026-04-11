import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useClaudeStore } from "@/stores/claudeStore";
import type {
  SessionReadyPayload,
  TextDeltaPayload,
  ToolStartPayload,
  ToolEndPayload,
  TurnCompletePayload,
  ErrorPayload,
  SessionEndPayload,
} from "@/types/claude";

/**
 * Subscribe to all Claude Tauri events and dispatch to the store.
 * Call this once at the ChatView level.
 */
export function useClaudeStream(): void {
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      const store = useClaudeStore.getState;

      unlisteners.push(
        await listen<SessionReadyPayload>("claude:session-ready", (event) => {
          store().setSessionReady(
            event.payload.session_id,
            event.payload.tools,
            event.payload.model
          );
        })
      );

      unlisteners.push(
        await listen<TextDeltaPayload>("claude:text-delta", (event) => {
          store().addTextDelta(
            event.payload.message_id,
            event.payload.text
          );
        })
      );

      unlisteners.push(
        await listen<ToolStartPayload>("claude:tool-start", (event) => {
          store().startTool(
            event.payload.tool_id,
            event.payload.tool_name,
            event.payload.friendly_label
          );
        })
      );

      unlisteners.push(
        await listen<ToolEndPayload>("claude:tool-end", (event) => {
          store().endTool(
            event.payload.tool_id,
            event.payload.success,
            event.payload.summary
          );
        })
      );

      unlisteners.push(
        await listen<TurnCompletePayload>("claude:turn-complete", (event) => {
          store().completeTurn(
            event.payload.cost_usd,
            event.payload.duration_ms
          );
        })
      );

      unlisteners.push(
        await listen<ErrorPayload>("claude:error", (event) => {
          store().setError(event.payload.message);
        })
      );

      unlisteners.push(
        await listen<SessionEndPayload>("claude:session-ended", (event) => {
          store().setDisconnected(event.payload.reason);
        })
      );

      unlisteners.push(
        await listen<SessionEndPayload>("claude:session-crashed", (event) => {
          store().setError(
            `Session crashed: ${event.payload.reason}`
          );
        })
      );
    };

    setup();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);
}
