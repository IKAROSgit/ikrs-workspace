import { useEffect, useMemo, useState, useCallback } from "react";
import { Command } from "cmdk";
import Fuse from "fuse.js";
import {
  Calendar,
  FileText,
  FolderOpen,
  Mail,
  MessageSquare,
  Settings,
  CheckSquare,
  RefreshCw,
  Search,
} from "lucide-react";
import { useEngagementStore } from "@/stores/engagementStore";
import { useUiStore } from "@/stores/uiStore";
import type { ViewId } from "@/Router";
import { useClaudeStore } from "@/stores/claudeStore";
import { useGmail } from "@/hooks/useGmail";
import { useCalendar } from "@/hooks/useCalendar";
import { useDrive } from "@/hooks/useDrive";
import { useNotes } from "@/hooks/useNotes";

/**
 * Global cmd+K / ctrl+K command palette.
 *
 * Design principles (2026-04-20):
 *  - ALWAYS available (mounted at app root, listens for the
 *    keyboard shortcut globally — no per-view wiring needed)
 *  - Navigation first (jump to any view in 2 keystrokes)
 *  - Surface search second (find an email / event / file / note by
 *    fuzzy name match over what's already loaded — no extra API
 *    call, sub-millisecond response)
 *  - Actions third (refresh, switch engagement, open settings)
 *  - Engagement-scoped — only searches the currently active
 *    engagement's data (prevents cross-client leakage)
 *
 * Non-goals:
 *  - Full-text server-side search across Gmail (would require a
 *    separate `q=` call; keep that for an explicit search view)
 *  - AI-authored commands (Claude lives in its own view)
 */
export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const setView = useUiStore((s) => s.setActiveView);
  const currentView = useUiStore((s) => s.activeView);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);
  const setActiveEngagement = useEngagementStore(
    (s) => s.setActiveEngagement,
  );

  const claudeSessionId = useClaudeStore((s) => s.sessionId);

  // Pull data from the four surface hooks. They're already loaded
  // elsewhere in the app; the hook instances here just re-read the
  // same zustand stores / server-side caches — cheap.
  const { emails, refresh: refreshGmail } = useGmail();
  const { events, refresh: refreshCalendar } = useCalendar();
  const { files: driveFiles, refresh: refreshDrive } = useDrive();
  const { files: vaultFiles, refresh: refreshNotes } = useNotes();

  // Global shortcut — cmd+K on mac, ctrl+K elsewhere.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      }
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // Reset query when closing.
  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  // Fuzzy index: rebuild only when underlying lists change. Fuse
  // index construction is ~1ms for lists of this size so we don't
  // bother caching across renders.
  const searchResults = useMemo(() => {
    if (!query.trim()) return { emails: [], events: [], drive: [], notes: [] };
    const q = query.trim();

    const emailFuse = new Fuse(emails, {
      keys: ["subject", "from", "snippet"],
      threshold: 0.4,
      includeScore: true,
    });
    const eventFuse = new Fuse(events, {
      keys: ["summary", "location", "attendees"],
      threshold: 0.4,
    });
    const driveFuse = new Fuse(driveFiles, {
      keys: ["name"],
      threshold: 0.4,
    });
    const noteFuse = new Fuse(vaultFiles, {
      keys: ["name", "rel_path"],
      threshold: 0.4,
    });

    return {
      emails: emailFuse.search(q).slice(0, 5).map((r) => r.item),
      events: eventFuse.search(q).slice(0, 5).map((r) => r.item),
      drive: driveFuse.search(q).slice(0, 5).map((r) => r.item),
      notes: noteFuse.search(q).slice(0, 5).map((r) => r.item),
    };
  }, [query, emails, events, driveFiles, vaultFiles]);

  const close = useCallback(() => setOpen(false), []);
  const go = useCallback(
    (view: ViewId) => {
      setView(view);
      close();
    },
    [setView, close],
  );

  const engagementLabel = (eid: string) => {
    const eng = engagements.find((e) => e.id === eid);
    if (!eng) return eid;
    const client = clients.find((c) => c.id === eng.clientId);
    const name = client?.name ?? "Unknown client";
    return eng.settings.description
      ? `${name} — ${eng.settings.description}`
      : name;
  };

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-background/70 backdrop-blur-sm"
      onClick={close}
    >
      <div
        className="w-full max-w-xl rounded-lg border border-border bg-popover shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <Command
          label="Global command palette"
          className="[&_[cmdk-input]]:outline-none"
          shouldFilter={false}
        >
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
            <Search size={14} className="text-muted-foreground" />
            <Command.Input
              value={query}
              onValueChange={setQuery}
              placeholder="Jump to anything — navigate, search, act…"
              className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
              autoFocus
            />
            <kbd className="text-[10px] px-1.5 py-0.5 bg-muted rounded border border-border text-muted-foreground">
              ESC
            </kbd>
          </div>
          <Command.List className="max-h-96 overflow-y-auto p-1">
            <Command.Empty className="px-3 py-6 text-center text-sm text-muted-foreground">
              {activeEngagementId
                ? "Nothing matches that."
                : "Select an engagement first."}
            </Command.Empty>

            {/* Navigation */}
            <Command.Group
              heading="Navigate"
              className="px-2 py-1 [&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1"
            >
              <NavItem
                icon={MessageSquare}
                label="Claude Chat"
                hint={claudeSessionId ? "Connected" : "Idle"}
                current={currentView === "claude"}
                onSelect={() => go("claude")}
              />
              <NavItem
                icon={Mail}
                label="Inbox"
                hint={`${emails.length} emails`}
                current={currentView === "inbox"}
                onSelect={() => go("inbox")}
              />
              <NavItem
                icon={Calendar}
                label="Calendar"
                hint={`${events.length} upcoming`}
                current={currentView === "calendar"}
                onSelect={() => go("calendar")}
              />
              <NavItem
                icon={FolderOpen}
                label="Drive Files"
                hint={`${driveFiles.length} files`}
                current={currentView === "files"}
                onSelect={() => go("files")}
              />
              <NavItem
                icon={FileText}
                label="Notes (Vault)"
                hint={`${vaultFiles.length} items`}
                current={currentView === "notes"}
                onSelect={() => go("notes")}
              />
              <NavItem
                icon={CheckSquare}
                label="Tasks"
                current={currentView === "tasks"}
                onSelect={() => go("tasks")}
              />
              <NavItem
                icon={Settings}
                label="Settings"
                current={currentView === "settings"}
                onSelect={() => go("settings")}
              />
            </Command.Group>

            {/* Engagement switching (only if multiple) */}
            {engagements.length > 1 && (
              <Command.Group
                heading="Switch Engagement"
                className="px-2 py-1 [&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1"
              >
                {engagements.map((e) => (
                  <Command.Item
                    key={e.id}
                    value={`switch-${e.id}-${engagementLabel(e.id)}`}
                    onSelect={() => {
                      setActiveEngagement(e.id);
                      close();
                    }}
                    className="flex items-center gap-2 px-2 py-1.5 rounded text-sm cursor-pointer aria-selected:bg-accent"
                  >
                    <span className="flex-1 truncate">
                      {engagementLabel(e.id)}
                    </span>
                    {e.id === activeEngagementId && (
                      <span className="text-[10px] text-muted-foreground">
                        current
                      </span>
                    )}
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {/* Search results — only render when query non-empty */}
            {query.trim() && (
              <>
                {searchResults.emails.length > 0 && (
                  <ResultGroup heading="Emails" icon={Mail}>
                    {searchResults.emails.map((m) => (
                      <Command.Item
                        key={`email-${m.id}`}
                        value={`email-${m.id}-${m.subject}`}
                        onSelect={() => go("inbox")}
                        className="flex flex-col px-2 py-1.5 rounded cursor-pointer aria-selected:bg-accent"
                      >
                        <span className="text-sm truncate">{m.subject}</span>
                        <span className="text-xs text-muted-foreground truncate">
                          {m.from}
                        </span>
                      </Command.Item>
                    ))}
                  </ResultGroup>
                )}
                {searchResults.events.length > 0 && (
                  <ResultGroup heading="Events" icon={Calendar}>
                    {searchResults.events.map((e) => (
                      <Command.Item
                        key={`event-${e.id}`}
                        value={`event-${e.id}-${e.summary}`}
                        onSelect={() => go("calendar")}
                        className="flex flex-col px-2 py-1.5 rounded cursor-pointer aria-selected:bg-accent"
                      >
                        <span className="text-sm truncate">{e.summary}</span>
                        <span className="text-xs text-muted-foreground truncate">
                          {e.start}
                          {e.location ? ` · ${e.location}` : ""}
                        </span>
                      </Command.Item>
                    ))}
                  </ResultGroup>
                )}
                {searchResults.drive.length > 0 && (
                  <ResultGroup heading="Drive" icon={FolderOpen}>
                    {searchResults.drive.map((f) => (
                      <Command.Item
                        key={`drive-${f.id}`}
                        value={`drive-${f.id}-${f.name}`}
                        onSelect={() => {
                          if (f.webViewLink) {
                            window.open(
                              f.webViewLink,
                              "_blank",
                              "noopener,noreferrer",
                            );
                          } else {
                            go("files");
                          }
                          close();
                        }}
                        className="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer aria-selected:bg-accent"
                      >
                        <span className="text-sm truncate flex-1">
                          {f.name}
                        </span>
                      </Command.Item>
                    ))}
                  </ResultGroup>
                )}
                {searchResults.notes.length > 0 && (
                  <ResultGroup heading="Notes" icon={FileText}>
                    {searchResults.notes.map((n) => (
                      <Command.Item
                        key={`note-${n.path}`}
                        value={`note-${n.path}-${n.name}`}
                        onSelect={() => go("notes")}
                        className="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer aria-selected:bg-accent"
                      >
                        <span className="text-sm truncate flex-1">
                          {n.rel_path}
                        </span>
                      </Command.Item>
                    ))}
                  </ResultGroup>
                )}
              </>
            )}

            {/* Actions */}
            <Command.Group
              heading="Actions"
              className="px-2 py-1 [&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1"
            >
              <ActionItem
                icon={RefreshCw}
                label="Refresh all surfaces"
                onSelect={() => {
                  refreshGmail();
                  refreshCalendar();
                  refreshDrive();
                  refreshNotes();
                  close();
                }}
              />
            </Command.Group>
          </Command.List>
        </Command>
      </div>
    </div>
  );
}

function NavItem({
  icon: Icon,
  label,
  hint,
  current,
  onSelect,
}: {
  icon: React.ComponentType<{ size?: number; className?: string }>;
  label: string;
  hint?: string;
  current?: boolean;
  onSelect: () => void;
}) {
  return (
    <Command.Item
      value={`nav-${label}`}
      onSelect={onSelect}
      className="flex items-center gap-2 px-2 py-1.5 rounded text-sm cursor-pointer aria-selected:bg-accent"
    >
      <Icon size={14} className="text-muted-foreground" />
      <span className="flex-1">{label}</span>
      {hint && (
        <span className="text-[10px] text-muted-foreground">{hint}</span>
      )}
      {current && (
        <span className="text-[10px] text-muted-foreground">current</span>
      )}
    </Command.Item>
  );
}

function ActionItem({
  icon: Icon,
  label,
  onSelect,
}: {
  icon: React.ComponentType<{ size?: number; className?: string }>;
  label: string;
  onSelect: () => void;
}) {
  return (
    <Command.Item
      value={`action-${label}`}
      onSelect={onSelect}
      className="flex items-center gap-2 px-2 py-1.5 rounded text-sm cursor-pointer aria-selected:bg-accent"
    >
      <Icon size={14} className="text-muted-foreground" />
      <span className="flex-1">{label}</span>
    </Command.Item>
  );
}

function ResultGroup({
  heading,
  icon: Icon,
  children,
}: {
  heading: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  children: React.ReactNode;
}) {
  return (
    <Command.Group
      heading={heading}
      className="px-2 py-1 [&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1 [&_[cmdk-group-heading]]:flex [&_[cmdk-group-heading]]:items-center [&_[cmdk-group-heading]]:gap-1.5"
    >
      {children}
      {/* heading icon decoration — cmdk doesn't expose an icon slot */}
      <div className="hidden">
        <Icon size={12} />
      </div>
    </Command.Group>
  );
}
