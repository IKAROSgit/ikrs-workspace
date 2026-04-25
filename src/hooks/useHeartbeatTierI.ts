/**
 * Tier I heartbeat hook (Phase E.7).
 *
 * Subscribes to the Rust-side `heartbeat:tier-i:tick` event (emitted by
 * `src-tauri/src/heartbeat/tick.rs`) and runs the JS-side reconciliation
 * each tick:
 *
 *  1. Read the most-recent N `heartbeat_health` docs for this tenant
 *     + engagement.
 *  2. Compute a verdict (most recent tick's status, lag since last tick,
 *     error_code if any).
 *  3. Surface to consumers via the hook return value — the Settings UI
 *     (E.8) will render this as a status pill + banner.
 *
 * The Rust side is just the cadence driver; all reads + reasoning live
 * here so we can iterate quickly without recompiling Tauri.
 *
 * "Run now" — call `runNow()` from the hook to trigger an immediate
 * tick. The Rust loop receives the notify, fires the event with
 * trigger="manual", and the listener here picks it up like any other
 * tick.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  collection,
  limit,
  onSnapshot,
  orderBy,
  query,
  where,
} from "firebase/firestore";

import { db as firestore } from "../lib/firebase";

/** Tier I verdict for the UI. */
export type TierIVerdict =
  | "healthy"
  | "stale" // Tier II hasn't run in over 2× the expected interval (>2h).
  | "error" // most recent Tier II tick reported error_code.
  | "unknown"; // no telemetry yet, or first run.

export interface HeartbeatTierIStatus {
  verdict: TierIVerdict;
  lastTickTs: string | null;
  lastTickStatus: string | null;
  lastErrorCode: string | null;
  /** ISO-8601 of when this Tier I JS-side tick fired. */
  lastTierIRunAt: string | null;
  /** Trigger for the most recent tick: "scheduled" | "manual" | null. */
  lastTrigger: "scheduled" | "manual" | null;
  /** Monotonic counter from the Rust side. */
  tickCount: number;
}

interface TierITickPayload {
  tick_ts: string;
  tick_count: number;
  trigger: "scheduled" | "manual";
}

interface HeartbeatHealthDoc {
  tenantId: string;
  engagementId: string;
  tier: "I" | "II";
  tickTs: string;
  status: "ok" | "error" | "skipped" | "no-op";
  durationMs: number;
  tokensUsed: number;
  promptVersion: string;
  actionsEmitted: number;
  errorCode: string | null;
  expiresAt: string;
}

const STALENESS_THRESHOLD_MS = 2 * 60 * 60 * 1000; // 2 hours
// We only ever read docs[0]; keep the listener payload tiny so Firestore
// quota stays small and the snapshot delivery is fast.
const TELEMETRY_QUERY_LIMIT = 1;

const INITIAL_STATUS: HeartbeatTierIStatus = {
  verdict: "unknown",
  lastTickTs: null,
  lastTickStatus: null,
  lastErrorCode: null,
  lastTierIRunAt: null,
  lastTrigger: null,
  tickCount: 0,
};

export function useHeartbeatTierI(
  tenantId: string | null,
  engagementId: string | null
): {
  status: HeartbeatTierIStatus;
  runNow: () => Promise<void>;
} {
  const [status, setStatus] = useState<HeartbeatTierIStatus>(INITIAL_STATUS);
  const [latestHealthDoc, setLatestHealthDoc] = useState<HeartbeatHealthDoc | null>(
    null
  );

  // Firestore listener: latest N heartbeat_health docs for this engagement.
  // Live-updates the hook's `latestHealthDoc` whenever Tier II writes a new
  // tick.
  useEffect(() => {
    if (!tenantId || !engagementId) {
      setLatestHealthDoc(null);
      return;
    }
    const q = query(
      collection(firestore, "heartbeat_health"),
      where("tenantId", "==", tenantId),
      where("engagementId", "==", engagementId),
      orderBy("tickTs", "desc"),
      limit(TELEMETRY_QUERY_LIMIT)
    );
    const unsub = onSnapshot(
      q,
      (snap) => {
        const docs = snap.docs.map((d) => d.data() as HeartbeatHealthDoc);
        setLatestHealthDoc(docs[0] ?? null);
      },
      (err) => {
        console.warn("[heartbeat:tier-i] firestore listener error", err);
        setLatestHealthDoc(null);
      }
    );
    return unsub;
  }, [tenantId, engagementId]);

  // Tauri event listener: Rust-side tick events.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    (async () => {
      unlisten = await listen<TierITickPayload>(
        "heartbeat:tier-i:tick",
        (event) => {
          setStatus((prev) => ({
            ...prev,
            lastTierIRunAt: new Date().toISOString(),
            lastTrigger: event.payload.trigger,
            tickCount: event.payload.tick_count,
          }));
        }
      );
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Recompute verdict whenever a new health doc arrives or a Tier I tick
  // fires (the latter is what gives us "stale" detection — the time
  // between Tier II writes is meaningful).
  useEffect(() => {
    setStatus((prev) => {
      if (!latestHealthDoc) {
        return { ...prev, verdict: "unknown" };
      }
      const lastTickMs = Date.parse(latestHealthDoc.tickTs);
      const ageMs = Number.isFinite(lastTickMs) ? Date.now() - lastTickMs : Infinity;
      let verdict: TierIVerdict;
      if (ageMs > STALENESS_THRESHOLD_MS) {
        verdict = "stale";
      } else if (
        latestHealthDoc.status === "error" ||
        latestHealthDoc.errorCode !== null
      ) {
        verdict = "error";
      } else {
        verdict = "healthy";
      }
      return {
        ...prev,
        verdict,
        lastTickTs: latestHealthDoc.tickTs,
        lastTickStatus: latestHealthDoc.status,
        lastErrorCode: latestHealthDoc.errorCode,
      };
    });
  }, [latestHealthDoc, status.tickCount]);

  const runNow = useCallback(async () => {
    try {
      await invoke<number>("heartbeat_run_now");
    } catch (err) {
      console.warn("[heartbeat:tier-i] run-now failed", err);
    }
  }, []);

  return useMemo(() => ({ status, runNow }), [status, runNow]);
}
