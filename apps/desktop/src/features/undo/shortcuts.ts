import { useEffect } from "react";
import { runUndo } from "./actions";

export type UndoShortcut = "undo" | "redo" | null;

/** `Ctrl/Cmd+Z` → undo; `Ctrl/Cmd+Shift+Z` (and `Ctrl+Y` outside macOS) → redo. */
export function undoShortcut(e: KeyboardEvent, isMac: boolean): UndoShortcut {
  const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!mod || e.altKey) return null;
  const key = e.key.toLowerCase();
  if (e.code === "KeyZ" || key === "z") return e.shiftKey ? "redo" : "undo";
  if (!isMac && !e.shiftKey && (e.code === "KeyY" || key === "y")) return "redo";
  return null;
}

/**
 * The key belongs to whatever has focus: text fields, Monaco, the terminal and anything inside
 * an open dialog keep their own undo.
 */
export function ownsUndoKeys(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return !!target.closest(
    'input, textarea, select, [contenteditable=""], [contenteditable="true"], .monaco-editor, .xterm, [role="dialog"], [role="menu"]',
  );
}

/** Installs the undo / redo shortcuts for the active repository. */
export function useUndoShortcuts(repoId: number | null) {
  useEffect(() => {
    if (repoId === null) return;
    const isMac = navigator.userAgent.includes("Mac");
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.repeat) return;
      const which = undoShortcut(e, isMac);
      if (!which || ownsUndoKeys(e.target) || ownsUndoKeys(document.activeElement)) return;
      e.preventDefault();
      void runUndo(repoId, which === "redo");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId]);
}
