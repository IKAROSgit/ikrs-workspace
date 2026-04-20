import { useState } from "react";
import { X, Send } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { sendGmailMessage } from "@/lib/tauri-commands";
import { useEngagementStore } from "@/stores/engagementStore";

/**
 * Compose / send-email modal.
 *
 * Uses the existing `gmail.modify` scope — no extra OAuth grant
 * needed. Response status from the Rust side is the same
 * discriminated-union pattern as list_gmail_inbox.
 *
 * Sends a plain-text body only for now. Rich HTML composition is a
 * follow-up; consultants typically reply with short text and Claude
 * already generates well-formatted plain prose.
 */
export function ComposeEmailModal({
  onClose,
  initialTo,
  initialSubject,
  initialBody,
}: {
  onClose: () => void;
  initialTo?: string;
  initialSubject?: string;
  initialBody?: string;
}) {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const [to, setTo] = useState(initialTo ?? "");
  const [cc, setCc] = useState("");
  const [subject, setSubject] = useState(initialSubject ?? "");
  const [body, setBody] = useState(initialBody ?? "");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState(false);

  const handleSend = async () => {
    if (!activeEngagementId) {
      setError("No active engagement.");
      return;
    }
    if (!to.trim()) {
      setError("At least one recipient required.");
      return;
    }
    setSending(true);
    setError(null);
    try {
      const r = await sendGmailMessage({
        engagementId: activeEngagementId,
        to: to.trim(),
        subject: subject.trim(),
        body,
        cc: cc.trim() || null,
      });
      switch (r.status) {
        case "ok":
          setSent(true);
          setTimeout(onClose, 800);
          break;
        case "not_connected":
          setError("Google not connected. Reconnect in Settings.");
          break;
        case "scope_missing":
          setError("Gmail send permission missing. Reconnect with the new scope.");
          break;
        case "rate_limited":
          setError("Rate limit reached. Try again in a minute.");
          break;
        case "network":
          setError("Network issue. Check your connection.");
          break;
        case "invalid":
          setError(r.message);
          break;
        case "other":
          setError(
            r.code
              ? `Gmail returned HTTP ${r.code}.`
              : "Unknown error sending message.",
          );
          break;
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  };

  const hasContent =
    to.trim() !== "" ||
    cc.trim() !== "" ||
    subject.trim() !== "" ||
    body.trim() !== "";

  const confirmClose = () => {
    if (sent || !hasContent) {
      onClose();
      return;
    }
    if (window.confirm("Discard this draft? Your message will be lost.")) {
      onClose();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm"
      // No auto-close on backdrop click — drafts must not be lost.
      // Codex 2026-04-20 data-loss guard. Cancel/X button confirms.
    >
      <div
        className="w-full max-w-2xl rounded-lg border border-border bg-popover shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-2 border-b border-border">
          <h3 className="text-sm font-semibold">New message</h3>
          <Button variant="ghost" size="sm" onClick={confirmClose}>
            <X size={14} />
          </Button>
        </div>
        <div className="p-4 space-y-3">
          <div className="grid grid-cols-[auto_1fr] items-center gap-2">
            <label className="text-xs text-muted-foreground">To</label>
            <Input
              value={to}
              onChange={(e) => setTo(e.target.value)}
              placeholder="someone@example.com"
              className="h-8 text-sm"
              disabled={sending || sent}
            />
            <label className="text-xs text-muted-foreground">Cc</label>
            <Input
              value={cc}
              onChange={(e) => setCc(e.target.value)}
              placeholder="optional"
              className="h-8 text-sm"
              disabled={sending || sent}
            />
            <label className="text-xs text-muted-foreground">Subject</label>
            <Input
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
              placeholder="Subject"
              className="h-8 text-sm"
              disabled={sending || sent}
            />
          </div>
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Write your message…"
            disabled={sending || sent}
            className="w-full h-64 text-sm p-3 bg-background border border-border rounded-md resize-none focus:outline-none focus:ring-2 focus:ring-primary"
          />
          {error && (
            <div className="text-sm text-destructive bg-destructive/10 p-2 rounded">
              {error}
            </div>
          )}
          {sent && (
            <div className="text-sm text-green-600 dark:text-green-400 bg-green-500/10 p-2 rounded">
              Sent.
            </div>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 px-4 py-2 border-t border-border">
          <Button variant="ghost" size="sm" onClick={confirmClose} disabled={sending}>
            Cancel
          </Button>
          <Button size="sm" onClick={handleSend} disabled={sending || sent}>
            <Send size={14} className="mr-1.5" />
            {sending ? "Sending…" : sent ? "Sent" : "Send"}
          </Button>
        </div>
      </div>
    </div>
  );
}
