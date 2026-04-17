import { useState, useEffect, useCallback } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { Button } from "@/components/ui/button";
import { isNewerVersion } from "@/lib/version-compare";

type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "error";

export function UpdateChecker() {
  const [version, setVersion] = useState<string>("");
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [errorMsg, setErrorMsg] = useState<string>("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion("unknown"));
  }, []);

  const checkForUpdates = useCallback(async (silent = false) => {
    if (!silent) setStatus("checking");
    setErrorMsg("");
    try {
      const result = await check();
      if (result) {
        // Layer 2 downgrade protection (Phase 4c §3).
        // Tauri's plugin-updater already compares versions before
        // handing back an Update object, so this branch is defensive:
        // a replayed or tampered manifest pointing at an older (but
        // validly-signed) bundle would be rejected here in the
        // unlikely event Tauri's own check was bypassed or regressed.
        // We also need a parseable `current` — if getVersion() is
        // "unknown" or absent, we refuse to update.
        const current = version || await getVersion().catch(() => "");
        if (!current || !isNewerVersion(result.version, current)) {
          // Silent reject: logging here would be noise, and showing
          // the user an error for what is effectively "no update"
          // would be worse UX than the standard "no update" path.
          setUpdate(null);
          if (!silent) setStatus("idle");
        } else {
          setUpdate(result);
          setStatus("available");
        }
      } else {
        setUpdate(null);
        if (!silent) setStatus("idle");
      }
    } catch {
      if (!silent) {
        setStatus("error");
        setErrorMsg("Failed to check for updates.");
      }
    }
  }, [version]);

  // Check for updates on mount (silent — errors ignored)
  useEffect(() => {
    checkForUpdates(true);
  }, [checkForUpdates]);

  const handleInstall = useCallback(async () => {
    if (!update) return;
    // Layer 2 final re-check just before install: belt-and-braces
    // against the user clicking Install on a stale state where
    // `update` was set earlier under different conditions.
    const current = version || await getVersion().catch(() => "");
    if (!current || !isNewerVersion(update.version, current)) {
      setUpdate(null);
      setStatus("idle");
      return;
    }
    setStatus("downloading");
    try {
      await update.downloadAndInstall();
    } catch {
      setStatus("error");
      setErrorMsg("Failed to install update.");
    }
  }, [update, version]);

  return (
    <div className="space-y-2">
      <p className="text-sm text-muted-foreground">
        App Version: {version || "..."}
      </p>

      {status === "available" && update && (
        <div className="flex items-center gap-2">
          <span className="text-sm text-green-600 dark:text-green-400">
            Update available: v{update.version}
          </span>
          <Button size="sm" onClick={handleInstall}>
            Install &amp; Restart
          </Button>
        </div>
      )}

      {status === "downloading" && (
        <p className="text-sm text-muted-foreground">
          Downloading update...
        </p>
      )}

      {status === "error" && (
        <p className="text-sm text-destructive">{errorMsg}</p>
      )}

      {(status === "idle" || status === "error") && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => checkForUpdates(false)}
        >
          Check for Updates
        </Button>
      )}

      {status === "checking" && (
        <p className="text-sm text-muted-foreground">Checking...</p>
      )}
    </div>
  );
}
