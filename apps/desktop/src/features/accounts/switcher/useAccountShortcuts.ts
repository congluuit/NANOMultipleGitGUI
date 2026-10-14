import { useEffect } from "react";
import { bindNthAccount } from "./actions";
import { accountShortcutIndex } from "./model";

/** Handles one keydown: `Ctrl/Cmd+Alt+N` binds the active repo to the Nth account. */
export function handleAccountShortcut(e: KeyboardEvent, isMac: boolean): boolean {
  const index = accountShortcutIndex(
    {
      code: e.code,
      ctrlKey: e.ctrlKey,
      altKey: e.altKey,
      metaKey: e.metaKey,
      shiftKey: e.shiftKey,
      altGraph: e.getModifierState?.("AltGraph") ?? false,
    },
    isMac,
  );
  if (index === null) return false;
  e.preventDefault();
  void bindNthAccount(index);
  return true;
}

/** Installs the `Ctrl/Cmd+Alt+1..9` account shortcuts for the whole window. */
export function useAccountShortcuts() {
  useEffect(() => {
    const isMac = navigator.userAgent.includes("Mac");
    const onKey = (e: KeyboardEvent) => void handleAccountShortcut(e, isMac);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
