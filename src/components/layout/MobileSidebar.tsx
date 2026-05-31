"use client";

import * as React from "react";
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { Menu, X } from "lucide-react";

import { EngagementSwitcher } from "./EngagementSwitcher";
import { NAV_ITEMS } from "./navItems";
import type { ViewId } from "@/Router";

interface MobileSidebarProps {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
  onCreateNew: () => void;
}

/**
 * Below the `md` breakpoint we hide the fixed `w-14` SideRail and
 * expose a hamburger that opens this drawer. The drawer mirrors the
 * desktop rail's content (EngagementSwitcher + nav) but uses an
 * icon-plus-label list because there's room and tooltips don't work
 * well on touch.
 *
 * Navigation auto-closes the drawer so the user lands on the chosen
 * view without a stray overlay.
 */
export function MobileSidebar({
  activeView,
  onNavigate,
  onCreateNew,
}: MobileSidebarProps) {
  const [open, setOpen] = React.useState(false);

  const handleNavigate = (view: ViewId) => {
    onNavigate(view);
    setOpen(false);
  };

  const handleCreateNew = () => {
    onCreateNew();
    setOpen(false);
  };

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Trigger
        render={
          <button
            className="p-2 -ml-2 rounded-md text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-2"
            aria-label="Open navigation"
          >
            <Menu size={18} />
          </button>
        }
      />
      <DialogPrimitive.Portal>
        <DialogPrimitive.Backdrop className="fixed inset-0 z-50 bg-black/30 supports-backdrop-filter:backdrop-blur-xs data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0" />
        <DialogPrimitive.Popup className="fixed inset-y-0 left-0 z-50 flex flex-col w-64 bg-sidebar border-r border-border outline-none duration-200 data-open:animate-in data-open:fade-in-0 data-open:slide-in-from-left-full data-closed:animate-out data-closed:fade-out-0 data-closed:slide-out-to-left-full">
          <DialogPrimitive.Title className="sr-only">
            Navigation
          </DialogPrimitive.Title>

          <div className="flex items-center justify-between px-3 py-2 border-b border-border">
            <span className="text-sm font-semibold">IKAROS Workspace</span>
            <DialogPrimitive.Close
              render={
                <button
                  className="p-1 rounded-md text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-2"
                  aria-label="Close navigation"
                >
                  <X size={16} />
                </button>
              }
            />
          </div>

          <div className="p-2 border-b border-border">
            <EngagementSwitcher onCreateNew={handleCreateNew} />
          </div>

          <nav className="flex flex-col gap-1 p-2">
            {NAV_ITEMS.map(({ id, icon: Icon, label }) => {
              const active = activeView === id;
              return (
                <button
                  key={id}
                  onClick={() => handleNavigate(id)}
                  aria-current={active ? "page" : undefined}
                  className={`flex items-center gap-3 h-10 px-3 rounded-lg text-sm transition-colors ${
                    active
                      ? "bg-primary text-primary-foreground"
                      : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                  }`}
                >
                  <Icon size={18} />
                  <span>{label}</span>
                </button>
              );
            })}
          </nav>
        </DialogPrimitive.Popup>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
