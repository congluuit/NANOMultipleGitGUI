import { useEffect } from "react";
import { reopenLastClosed } from "@/features/repos/actions";
import { useWorkspace } from "@/stores/workspace";
import { displayedTabs } from "./pinnedTabs";

export type TabShortcut = "next" | "prev" | "reopen";

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/**
 * `Ctrl+Tab` / `Ctrl+Shift+Tab` cycle the repository tabs (on every platform, as in browsers);
 * `Ctrl/Cmd+Shift+T` reopens the tab closed last. Not taken from the integrated terminal.
 */
export function tabShortcut(e: KeyLike, isMac: boolean): TabShortcut | null {
  if (e.altKey) return null;
  if (e.code === "Tab" && e.ctrlKey && !e.metaKey) return e.shiftKey ? "prev" : "next";
  const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (mod && e.shiftKey && e.code === "KeyT") return "reopen";
  return null;
}

export function tabShortcutLabel(shortcut: TabShortcut, isMac: boolean): string {
  if (shortcut === "reopen") return isMac ? "⌘⇧T" : "Ctrl+Shift+T";
  return shortcut === "next" ? "Ctrl+Tab" : "Ctrl+Shift+Tab";
}

/** The tab `step` places after the active one (wrapping around); `null` with fewer than 2 tabs. */
export function neighbourTab(step: 1 | -1): number | null {
  const { activeId } = useWorkspace.getState();
  const tabs = displayedTabs(useWorkspace.getState().tabs);
  if (tabs.length < 2) return null;
  const index = Math.max(
    0,
    tabs.findIndex((tab) => tab.id === activeId),
  );
  return tabs[(index + step + tabs.length) % tabs.length]?.id ?? null;
}

export function useTabShortcuts() {
  useEffect(() => {
    const isMac = navigator.userAgent.includes("Mac");
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.repeat) return;
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      const shortcut = tabShortcut(e, isMac);
      if (!shortcut) return;
      e.preventDefault();
      if (shortcut === "reopen") {
        void reopenLastClosed();
        return;
      }
      const id = neighbourTab(shortcut === "next" ? 1 : -1);
      if (id !== null) useWorkspace.getState().setActive(id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
