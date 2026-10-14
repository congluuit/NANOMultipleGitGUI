import { useEffect } from "react";
import { useTerminal } from "./store";

/** `Ctrl+`` (layout independent: the physical Backquote key) toggles the terminal panel. */
export const isTerminalShortcut = (e: KeyboardEvent) =>
  e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey && e.code === "Backquote";

/** Installs the toggle shortcut while a usable repo (`repoId`) is active. */
export function useTerminalShortcut(repoId: number | null) {
  useEffect(() => {
    if (repoId === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (!isTerminalShortcut(e)) return;
      e.preventDefault();
      useTerminal.getState().toggle(repoId);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId]);
}
