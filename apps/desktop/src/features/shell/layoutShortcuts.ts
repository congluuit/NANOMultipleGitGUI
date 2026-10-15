import { useEffect } from "react";
import { useLayout, type PanelId } from "@/stores/layout";

/**
 * `Ctrl/Cmd+B` toggles the sidebar, `Ctrl/Cmd+Alt+B` the right panel (as in VS Code).
 * Not taken from the integrated terminal, where Ctrl+B is readline / tmux.
 */
export function layoutShortcut(e: KeyboardEvent, isMac: boolean): PanelId | null {
  const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!mod || e.shiftKey || e.code !== "KeyB") return null;
  if (e.target instanceof Element && e.target.closest(".xterm")) return null;
  return e.altKey ? "right" : "sidebar";
}

export function shortcutLabel(id: PanelId, isMac: boolean): string {
  const mod = isMac ? "⌘" : "Ctrl+";
  const alt = isMac ? "⌥" : "Alt+";
  return id === "sidebar" ? `${mod}B` : `${mod}${alt}B`;
}

export function useLayoutShortcuts() {
  useEffect(() => {
    const isMac = navigator.userAgent.includes("Mac");
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.repeat) return;
      const id = layoutShortcut(e, isMac);
      if (!id) return;
      e.preventDefault();
      useLayout.getState().togglePanel(id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
