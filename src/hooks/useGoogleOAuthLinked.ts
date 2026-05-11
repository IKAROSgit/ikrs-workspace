import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCredential, makeKeychainKey } from "@/lib/tauri-commands";

/**
 * Reports whether the given engagement has a stored Google OAuth
 * token in the macOS keychain. Returns `null` while the initial
 * check is in flight so callers can avoid flashing "not linked"
 * on first paint.
 *
 * Re-runs the check whenever the engagement changes or the Rust
 * side fires `oauth:token-stored` (after a successful connect),
 * keeping the StatusBar, Settings card, and any other consumers
 * coherent without duplicating the keychain lookup.
 */
export function useGoogleOAuthLinked(
  engagementId: string | null,
): boolean | null {
  const [linked, setLinked] = useState<boolean | null>(null);

  useEffect(() => {
    if (!engagementId) {
      setLinked(false);
      return;
    }
    let cancelled = false;

    const check = async () => {
      try {
        const key = makeKeychainKey(engagementId, "google");
        const value = await getCredential(key);
        if (!cancelled) setLinked(!!value);
      } catch {
        if (!cancelled) setLinked(false);
      }
    };

    void check();

    let unlisten: (() => void) | undefined;
    void listen("oauth:token-stored", () => {
      void check();
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [engagementId]);

  return linked;
}
