import { useState, useEffect, useCallback } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { Button } from "@/components/ui/button";

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
        setUpdate(result);
        setStatus("available");
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
  }, []);

  // Check for updates on mount (silent — errors ignored)
  useEffect(() => {
    checkForUpdates(true);
  }, [checkForUpdates]);

  const handleInstall = useCallback(async () => {
    if (!update) return;
    setStatus("downloading");
    try {
      await update.downloadAndInstall();
    } catch {
      setStatus("error");
      setErrorMsg("Failed to install update.");
    }
  }, [update]);

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
