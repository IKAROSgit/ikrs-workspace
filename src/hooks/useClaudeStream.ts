import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useClaudeStore } from "@/stores/claudeStore";
import { useMcpStore } from "@/stores/mcpStore";
import { extractMcpServers } from "@/lib/mcp-utils";
import type {
  SessionReadyPayload,
  TextDeltaPayload,
  ToolStartPayload,
  ToolEndPayload,
  TurnCompletePayload,
  ErrorPayload,
  SessionEndPayload,
  McpAuthErrorPayload,
  WriteVerificationPayload,
} from "@/types/claude";

/**
 * Module-level Tauri listener registration. Diagnosed 2026-04-19 via
 * direct Mac SSH: the previous implementation registered listeners
 * inside a ChatView `useEffect` with `await listen(...)`. `useEffect`
 * for `useWorkspaceSession` (which calls `spawnClaudeSession`) runs
 * in parallel. By the time the listener registration IPC round-trips
 * complete, Claude CLI has already emitted its `system:init` frame
 * (Claude's init is synthesized locally — emitted within ~tens of
 * ms, before the model is even queried). The `claude:session-ready`
 * event fires into the void; no listener is subscribed yet; the
 * frontend store stays in `status:"connecting"` forever while the
 * backend is fully connected with all MCPs happy. Classic
 * use-before-subscribe race for event-based state hydration.
 *
 * Fix: register once at module-import time. `listen()` completes its
 * subscription handshake before any component renders, so any event
 * emitted after the JS bundle loads is guaranteed to be received.
 * Tauri's `listen` returns an UnlistenFn but we intentionally never
 * unlisten — these subscriptions are supposed to live for the whole
 * app lifetime. The hook below is a thin compat shim so existing
 * `useClaudeStream()` callers in ChatView don't have to change.
 *
 * Unit-testing note: we guard registration behind `__TAURI_INTERNALS__`
 * so Vitest in a non-Tauri Node context can still import this module
 * without the listen() IPC attempt. In tests, the setup is a no-op.
 */
function registerListeners(): void {
  const store = useClaudeStore.getState;

  const register = (fn: () => Promise<unknown>) => {
    fn().catch((err) => {
      // If a listen() registration fails at startup, we're
      // effectively wedged anyway — surface it in the console so
      // Moe or a future debugger can spot it. Don't swallow.
      // eslint-disable-next-line no-console
      console.error("[useClaudeStream] listen() failed:", err);
    });
  };

  register(() =>
    listen<SessionReadyPayload>("claude:session-ready", (event) => {
      store().setSessionReady(
        event.payload.session_id,
        event.payload.tools,
        event.payload.model
      );
      const mcpServers = extractMcpServers(event.payload.tools);
      useMcpStore.getState().setServers(mcpServers);
    })
  );

  register(() =>
    listen<TextDeltaPayload>("claude:text-delta", (event) => {
      store().addTextDelta(event.payload.message_id, event.payload.text);
    })
  );

  register(() =>
    listen<ToolStartPayload>("claude:tool-start", (event) => {
      store().startTool(
        event.payload.tool_id,
        event.payload.tool_name,
        event.payload.friendly_label,
        event.payload.tool_input ?? undefined
      );
    })
  );

  register(() =>
    listen<ToolEndPayload>("claude:tool-end", (event) => {
      store().endTool(
        event.payload.tool_id,
        event.payload.success,
        event.payload.summary,
        event.payload.result_content ?? undefined
      );
    })
  );

  register(() =>
    listen<TurnCompletePayload>("claude:turn-complete", (event) => {
      store().completeTurn(event.payload.cost_usd, event.payload.duration_ms);
    })
  );

  register(() =>
    listen<ErrorPayload>("claude:error", (event) => {
      store().setError(event.payload.message);
    })
  );

  // session-id guarded lifecycle handlers.
  //
  // Why: the useClaudeStream module registers listeners once at load
  // and lives for the whole app session. Over that lifetime many
  // Claude subprocesses come and go — user switches engagements, I
  // kill-and-respawn during dev, the monitor task detects exits of
  // evicted sessions, etc. Each spawn emits its own session-ended /
  // session-crashed events from its own monitor_process task. Those
  // events carry the session_id of the dying process. Without a
  // guard, a late-arriving "session-crashed" from a PREVIOUS session
  // clobbers the live session's state in the store — UI flips to
  // status:"error" with "Session crashed: Claude CLI error" even
  // though the real current session is happily running.
  //
  // Diagnosed 2026-04-20: Moe sees "crashed" in the UI while the
  // Rust log shows the current session succeeded. The stale monitor
  // event from the killed-for-rebuild previous claude was overwriting
  // his fresh session's state.
  //
  // Fix: compare event.payload.session_id against the store's
  // current sessionId. Ignore if mismatch.
  register(() =>
    listen<SessionEndPayload>("claude:session-ended", (event) => {
      const current = useClaudeStore.getState().sessionId;
      if (current && event.payload.session_id !== current) {
        // eslint-disable-next-line no-console
        console.debug(
          "[useClaudeStream] ignoring stale session-ended for",
          event.payload.session_id,
          "(current=",
          current,
          ")"
        );
        return;
      }
      store().setDisconnected(event.payload.reason);
      useMcpStore.getState().setServers([]);
    })
  );

  register(() =>
    listen<SessionEndPayload>("claude:session-crashed", (event) => {
      const current = useClaudeStore.getState().sessionId;
      if (current && event.payload.session_id !== current) {
        // eslint-disable-next-line no-console
        console.debug(
          "[useClaudeStream] ignoring stale session-crashed for",
          event.payload.session_id,
          "(current=",
          current,
          ")"
        );
        return;
      }
      store().setError(`Session crashed: ${event.payload.reason}`);
    })
  );

  register(() =>
    listen<McpAuthErrorPayload>("claude:mcp-auth-error", (event) => {
      store().setAuthError(
        event.payload.server_name,
        event.payload.error_hint
      );
    })
  );

  // Ground-truth record of every Write/Edit/NotebookEdit Claude did
  // this session. Backend has already stat'd the file; this just
  // adds the result to the store for UI display. Lies (verified=false
  // but claude_claimed_success=true) are ALSO surfaced via
  // claude:error so the user can't miss them.
  register(() =>
    listen<WriteVerificationPayload>("claude:write-verified", (event) => {
      store().recordWriteVerification(event.payload);
    }),
  );
}

let listenersRegistered = false;
function ensureListenersRegistered(): void {
  if (listenersRegistered) return;
  // Only call listen() when running inside Tauri. In Vitest / SSR
  // contexts __TAURI_INTERNALS__ is undefined and the IPC would throw.
  if (typeof window === "undefined") return;
  if (!("__TAURI_INTERNALS__" in window)) return;
  listenersRegistered = true;
  registerListeners();
}

// Register as soon as this module is first imported inside a Tauri
// webview. This runs before any React component mounts, closing the
// use-before-subscribe race described above.
ensureListenersRegistered();

/**
 * Kept for API compat with the pre-2026-04-19 `useClaudeStream()`
 * call in ChatView. The real registration happens at module load;
 * this hook is a no-op belt-and-braces that ensures the module has
 * been imported (React tree-shakes dead imports in rare configs).
 */
export function useClaudeStream(): void {
  useEffect(() => {
    ensureListenersRegistered();
  }, []);
}
