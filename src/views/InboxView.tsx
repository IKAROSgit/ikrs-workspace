import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { RefreshCw, Mail, MailOpen } from "lucide-react";
import { useGmail } from "@/hooks/useGmail";
import { useEngagementStore } from "@/stores/engagementStore";

export default function InboxView() {
  const { emails, loading, error, isConnected, refresh } = useGmail();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  if (!activeEngagementId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Mail size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to view inbox.</p>
      </div>
    );
  }

  if (!isConnected) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Mail size={48} className="mb-4 opacity-50" />
        <p>Connect a Google account in Settings to view emails.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-4 py-2 border-b border-border">
        <h2 className="text-sm font-semibold">Inbox</h2>
        <Button variant="ghost" size="sm" onClick={refresh} disabled={loading}>
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
        </Button>
      </div>

      {error && (
        <div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
          {error}
        </div>
      )}

      <ScrollArea className="flex-1">
        {emails.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full py-12 text-muted-foreground">
            <MailOpen size={32} className="mb-2 opacity-50" />
            <p className="text-sm">No emails to display.</p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {emails.map((email) => (
              <div
                key={email.id}
                className={`flex flex-col gap-1 px-4 py-3 hover:bg-accent/50 cursor-pointer ${
                  !email.isRead ? "bg-accent/20" : ""
                }`}
              >
                <div className="flex items-center justify-between">
                  <span
                    className={`text-sm ${!email.isRead ? "font-semibold" : ""}`}
                  >
                    {email.from}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    {email.date}
                  </span>
                </div>
                <span className="text-sm">{email.subject}</span>
                <span className="text-xs text-muted-foreground truncate">
                  {email.snippet}
                </span>
              </div>
            ))}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
