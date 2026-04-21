import { useState, useCallback } from "react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import {
  killClaudeSession,
  spawnClaudeSession,
  sendClaudeMessage,
  getResumeSessionId,
  claudeVersionCheck,
  claudeAuthStatus,
  distillSessionMemory,
} from "@/lib/tauri-commands";
import { composeSessionBriefing } from "@/lib/briefing";
import type { ChatMessage } from "@/types/claude";

/**
 * Polls claudeStore.status via subscribe() with a timeout.
 * Returns true if the target status is reached, false on timeout.
 */
function waitForStatus(target: string, timeoutMs: number): Promise<boolean> {
  return new Promise((resolve) => {
    // Check immediately
    if (useClaudeStore.getState().status === target) {
      resolve(true);
      return;
    }
    const timer = setTimeout(() => {
      unsub();
      resolve(false);
    }, timeoutMs);
    const unsub = useClaudeStore.subscribe((state) => {
      if (state.status === target) {
        clearTimeout(timer);
        unsub();
        resolve(true);
      }
    });
  });
}

/**
 * Fire-and-forget session-boot briefing injection. Composes the
 * briefing markdown and sends it to Claude as a synthetic first-
 * user-message — the CLI processes it, emits system:init, and
 * streams back a proactive opener that the user sees in their
 * normal chat transcript. The briefing itself is NOT added to
 * `useClaudeStore.messages`, so the user never sees the raw data
 * dump they didn't type (only Claude's response to it).
 *
 * Intentionally silent on failure — if the briefing can't be built
 * (offline, APIs down, slug missing) we just let Claude cold-start.
 * Better to skip the briefing than block session boot.
 *
 * Called only on FRESH spawn paths, never on --resume; resumed
 * sessions have the prior conversation history, so injecting a
 * briefing would confuse the model with a repeated "here's today's
 * state" turn mid-thread.
 */
async function injectSessionBriefing(
  sessionId: string,
  engagementId: string,
  clientSlug: string | undefined,
): Promise<void> {
  let flippedToThinking = false;
  try {
    const briefing = await composeSessionBriefing(engagementId, clientSlug);
    if (briefing.trim().length === 0) return;
    // Only flip to "thinking" if this briefing is for the CURRENT
    // session. During rapid engagement switches or reconnects, the
    // session we primed may already have been replaced — we must
    // never steer another session's UI state.
    if (useClaudeStore.getState().sessionId !== sessionId) return;
    // Mirror the "thinking" state so the UI shows activity during
    // the first streamed response, matching what the user would see
    // after a normal send. Claude's `assistant` frames will flip
    // status back to "connected" via completeTurn.
    useClaudeStore.setState({ status: "thinking" });
    flippedToThinking = true;
    await sendClaudeMessage(sessionId, briefing);
  } catch (e) {
    // eslint-disable-next-line no-console
    console.warn("[briefing] failed, Claude will cold-start", e);
    // Codex 2026-04-21 pre-push: if we flipped to "thinking" and
    // the send failed before Claude's stream could flip us back via
    // completeTurn, the input stays disabled and the user is stuck
    // until they reconnect. Restore "connected" iff we're still
    // looking at the session we primed AND still in thinking.
    if (flippedToThinking) {
      const s = useClaudeStore.getState();
      if (s.sessionId === sessionId && s.status === "thinking") {
        useClaudeStore.setState({ status: "connected" });
      }
    }
  }
}

/**
 * Flatten the in-store chat transcript into a simple markdown
 * document the distiller can review. Excludes streaming partials
 * (isStreaming=true) and empty messages. The distiller does its own
 * interpretation — we don't need to pre-digest.
 */
function transcriptFromMessages(messages: ChatMessage[]): string {
  const lines: string[] = [];
  for (const m of messages) {
    if (m.isStreaming) continue;
    const text = (m.text ?? "").trim();
    if (!text) continue;
    const who = m.role === "user" ? "consultant" : "claude";
    const ts = m.timestamp instanceof Date
      ? m.timestamp.toISOString()
      : new Date(m.timestamp).toISOString();
    lines.push(`### ${who} · ${ts}\n\n${text}\n`);
  }
  return lines.join("\n");
}

/**
 * Fire the session-end distiller for the engagement we're leaving.
 * Awaited briefly (ms) because the Rust side detaches the actual
 * Claude CLI call into a tokio task and returns immediately — the
 * await here just confirms the request was accepted.
 *
 * Silent on any failure: distillation is a "nice to have" and must
 * never block a session switch / kill.
 */
async function fireSessionEndDistiller(
  clientSlug: string | undefined,
  messages: ChatMessage[],
): Promise<void> {
  if (!clientSlug) return;
  const transcript = transcriptFromMessages(messages);
  // Backend also guards on length, but we can short-circuit the IPC
  // round-trip when there's obviously nothing worth distilling.
  if (transcript.trim().length < 200) return;
  try {
    await distillSessionMemory(clientSlug, transcript);
  } catch (e) {
    // eslint-disable-next-line no-console
    console.warn("[distiller] request failed (non-fatal)", e);
  }
}

export function useWorkspaceSession() {
  const [switching, setSwitching] = useState(false);

  const connect = useCallback(async () => {
    if (!navigator.onLine) {
      useClaudeStore.getState().setError(
        "Unable to reach Claude. Check your internet connection and try again."
      );
      return;
    }

    const engagement = useEngagementStore.getState().engagements.find(
      (e) => e.id === useEngagementStore.getState().activeEngagementId
    );
    if (!engagement) return;

    // Preflight
    const version = await claudeVersionCheck();
    if (!version.installed) {
      useClaudeStore.getState().setError("Claude CLI not found. Please install Claude Code first.");
      return;
    }
    if (!version.meets_minimum) {
      useClaudeStore.getState().setError(`Claude CLI ${version.version} is too old. Please update to v2.1.0 or later.`);
      return;
    }
    const auth = await claudeAuthStatus();
    if (!auth.loggedIn) {
      useClaudeStore.getState().setError("Not signed in to Claude. Please sign in first from Settings.");
      return;
    }

    useClaudeStore.getState().reset();
    useClaudeStore.setState({ status: "connecting" });

    try {
      // Resolve client slug for MCP config generation
      const client = useEngagementStore.getState().clients.find(
        (c) => c.id === engagement.clientId
      );

      // Check for resume session
      const resumeId = await getResumeSessionId(engagement.id);
      const spawnedSessionId = await spawnClaudeSession(
        engagement.id,
        engagement.vault.path,
        resumeId ?? undefined,
        client?.slug,
        engagement.settings.strictMcp,
      );

      // Flip to connected using the session_id returned from spawn.
      // Diagnosed 2026-04-20: previously we waited for the
      // `claude:session-ready` event, which fires from the Rust
      // stream parser on `system:init`. But claude --print --input-
      // format stream-json does not emit init until the first user
      // message arrives on stdin — deadlock (user can't type until
      // connected; never connects without typing). The Rust side now
      // emits a synthetic session-ready on spawn, but we also
      // short-circuit here using the returned session_id as the
      // authoritative "session exists" signal. Tools + real model
      // populate from the real system:init when the user sends.
      if (useClaudeStore.getState().status !== "connected") {
        useClaudeStore.getState().setSessionReady(
          spawnedSessionId,
          [],
          "initializing",
        );
      }

      // Frontend-driven resume timeout (5s)
      if (resumeId) {
        const connected = await waitForStatus("connected", 5000);
        if (!connected) {
          // Resume failed — kill and retry without --resume
          const currentSessionId = useClaudeStore.getState().sessionId;
          if (currentSessionId) {
            await killClaudeSession(currentSessionId);
          }
          useClaudeStore.getState().reset();
          useClaudeStore.setState({ status: "connecting" });
          const retrySessionId = await spawnClaudeSession(
            engagement.id,
            engagement.vault.path,
            undefined,
            client?.slug,
            engagement.settings.strictMcp,
          );
          if (useClaudeStore.getState().status !== "connected") {
            useClaudeStore.getState().setSessionReady(
              retrySessionId,
              [],
              "initializing",
            );
          }
          // Retry path is effectively a fresh spawn — brief it.
          void injectSessionBriefing(
            retrySessionId,
            engagement.id,
            client?.slug,
          );
        }
        // Successful resume: do NOT brief. Conversation history is
        // already in Claude's context; a briefing would duplicate.
      } else {
        // Fresh spawn — inject the briefing so Claude opens
        // proactively instead of "what do you want to work on?".
        void injectSessionBriefing(
          spawnedSessionId,
          engagement.id,
          client?.slug,
        );
      }
    } catch (e) {
      useClaudeStore.getState().setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const switchEngagement = useCallback(async (newEngagementId: string) => {
    if (switching) return;

    if (!navigator.onLine) {
      useClaudeStore.getState().setError(
        "Unable to reach Claude. Check your internet connection and try again."
      );
      return;
    }

    setSwitching(true);

    try {
      // 1. Fire evolving-memory distiller for the engagement we're
      //    leaving. Must happen BEFORE killing Claude so the Rust
      //    side can still resolve the session context — the distiller
      //    call itself detaches a background task, so we're not
      //    blocked here. Also BEFORE saveAndClearHistory so the
      //    transcript is still in the store.
      const currentEngId = useEngagementStore.getState().activeEngagementId;
      if (currentEngId) {
        const currentClient = useEngagementStore.getState().clients.find((c) => {
          const eng = useEngagementStore
            .getState()
            .engagements.find((e) => e.id === currentEngId);
          return eng ? c.id === eng.clientId : false;
        });
        void fireSessionEndDistiller(
          currentClient?.slug,
          useClaudeStore.getState().messages,
        );
      }

      // 2. Kill current Claude session
      const currentSessionId = useClaudeStore.getState().sessionId;
      if (currentSessionId) {
        await killClaudeSession(currentSessionId);
      }

      // 3. Save current chat history
      if (currentEngId) {
        useClaudeStore.getState().saveAndClearHistory(currentEngId);
      }

      // 3. Set new active engagement
      useEngagementStore.getState().setActiveEngagement(newEngagementId);

      // 4. Load target engagement's chat history
      useClaudeStore.getState().loadHistory(newEngagementId);

      // 5. Check for resume session and spawn
      const resumeId = await getResumeSessionId(newEngagementId);
      const engagement = useEngagementStore.getState().engagements.find(
        (e) => e.id === newEngagementId
      );
      const switchClient = useEngagementStore.getState().clients.find(
        (c) => c.id === engagement?.clientId
      );
      if (engagement) {
        useClaudeStore.setState({ status: "connecting" });
        const switchSessionId = await spawnClaudeSession(
          newEngagementId,
          engagement.vault.path,
          resumeId ?? undefined,
          switchClient?.slug,
          engagement.settings.strictMcp,
        );
        // See comment in connect() above re: claude-print doesn't
        // emit system:init without user input → deadlock. Short-
        // circuit on the returned session_id.
        if (useClaudeStore.getState().status !== "connected") {
          useClaudeStore.getState().setSessionReady(
            switchSessionId,
            [],
            "initializing",
          );
        }

        // Frontend-driven resume timeout (5s)
        if (resumeId) {
          const connected = await waitForStatus("connected", 5000);
          if (!connected) {
            const sid = useClaudeStore.getState().sessionId;
            if (sid) await killClaudeSession(sid);
            useClaudeStore.getState().reset();
            useClaudeStore.setState({ status: "connecting" });
            const retryId = await spawnClaudeSession(
              newEngagementId,
              engagement.vault.path,
              undefined,
              switchClient?.slug,
              engagement.settings.strictMcp,
            );
            if (useClaudeStore.getState().status !== "connected") {
              useClaudeStore.getState().setSessionReady(
                retryId,
                [],
                "initializing",
              );
            }
            // Retry path is a fresh spawn — brief it.
            void injectSessionBriefing(
              retryId,
              newEngagementId,
              switchClient?.slug,
            );
          }
          // Successful resume: no briefing (see connect() for reasoning).
        } else {
          // Fresh spawn on the new engagement — brief.
          void injectSessionBriefing(
            switchSessionId,
            newEngagementId,
            switchClient?.slug,
          );
        }
      }
    } catch (e) {
      useClaudeStore.getState().setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSwitching(false);
    }
  }, [switching]);

  return { connect, switchEngagement, switching };
}
