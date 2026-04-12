import { useState, useCallback } from "react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";
import {
  killClaudeSession,
  spawnClaudeSession,
  getResumeSessionId,
  claudeVersionCheck,
  claudeAuthStatus,
} from "@/lib/tauri-commands";

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

export function useWorkspaceSession() {
  const [switching, setSwitching] = useState(false);

  const connect = useCallback(async () => {
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
      // Check for resume session
      const resumeId = await getResumeSessionId(engagement.id);
      await spawnClaudeSession(engagement.id, engagement.vault.path, resumeId ?? undefined);

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
          await spawnClaudeSession(engagement.id, engagement.vault.path);
        }
      }
    } catch (e) {
      useClaudeStore.getState().setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const switchEngagement = useCallback(async (newEngagementId: string) => {
    if (switching) return;
    setSwitching(true);

    try {
      // 1. Kill current Claude session
      const currentSessionId = useClaudeStore.getState().sessionId;
      if (currentSessionId) {
        await killClaudeSession(currentSessionId);
      }

      // 2. Save current chat history
      const currentEngId = useEngagementStore.getState().activeEngagementId;
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
      if (engagement) {
        useClaudeStore.setState({ status: "connecting" });
        await spawnClaudeSession(
          newEngagementId,
          engagement.vault.path,
          resumeId ?? undefined,
        );

        // Frontend-driven resume timeout (5s)
        if (resumeId) {
          const connected = await waitForStatus("connected", 5000);
          if (!connected) {
            const sid = useClaudeStore.getState().sessionId;
            if (sid) await killClaudeSession(sid);
            useClaudeStore.getState().reset();
            useClaudeStore.setState({ status: "connecting" });
            await spawnClaudeSession(newEngagementId, engagement.vault.path);
          }
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
