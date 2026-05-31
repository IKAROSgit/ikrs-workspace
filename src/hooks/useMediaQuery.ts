import { useEffect, useState } from "react";

/**
 * Subscribe to a CSS media query, with an SSR-safe initial value.
 *
 * Returns `true` whenever the query currently matches. Updates on
 * `change` events. Falls back to `false` when `window.matchMedia`
 * is unavailable (e.g. test environments without jsdom matchMedia).
 */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState<boolean>(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return false;
    }
    return window.matchMedia(query).matches;
  });

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const mql = window.matchMedia(query);
    const handler = (e: MediaQueryListEvent) => setMatches(e.matches);
    // Sync once on mount in case the query result changed between
    // initial useState evaluation and effect attachment.
    setMatches(mql.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, [query]);

  return matches;
}

/**
 * Convenience hook: true when the viewport is below Tailwind's `md`
 * breakpoint (768 px). Use to gate mobile-only UI (e.g. hamburger
 * drawer in place of the fixed side rail).
 *
 * Single source of truth for the "mobile" threshold — any view that
 * needs to branch on it should consume this, not hardcode the px
 * value, so a future breakpoint change is one edit.
 */
export function useIsMobile(): boolean {
  return useMediaQuery("(max-width: 767.98px)");
}
