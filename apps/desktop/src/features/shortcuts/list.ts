/** Every keyboard shortcut of the app, for the cheat sheet (`?`). Keep in sync with the handlers. */

export type ShortcutGroup = "general" | "tabs" | "repo";

export type ShortcutId =
  | "palette"
  | "cheatSheet"
  | "sidebar"
  | "rightPanel"
  | "terminal"
  | "nextTab"
  | "prevTab"
  | "reopenTab"
  | "closeTab"
  | "undo"
  | "redo"
  | "findCommit"
  | "commit"
  | "bindAccount";

export interface ShortcutEntry {
  /** i18n key under `shortcuts.item`. */
  id: ShortcutId;
  group: ShortcutGroup;
  /** Key combinations; one entry per alternative. */
  keys: (isMac: boolean) => string[];
}

const mod = (isMac: boolean) => (isMac ? "⌘" : "Ctrl");
const alt = (isMac: boolean) => (isMac ? "⌥" : "Alt");

export const SHORTCUTS: readonly ShortcutEntry[] = [
  { id: "palette", group: "general", keys: (m) => [`${mod(m)}+K`] },
  { id: "cheatSheet", group: "general", keys: () => ["?"] },
  { id: "sidebar", group: "general", keys: (m) => [`${mod(m)}+B`] },
  { id: "rightPanel", group: "general", keys: (m) => [`${mod(m)}+${alt(m)}+B`] },
  { id: "terminal", group: "general", keys: () => ["Ctrl+`"] },
  { id: "nextTab", group: "tabs", keys: () => ["Ctrl+Tab"] },
  { id: "prevTab", group: "tabs", keys: () => ["Ctrl+Shift+Tab"] },
  { id: "reopenTab", group: "tabs", keys: (m) => [`${mod(m)}+Shift+T`] },
  { id: "closeTab", group: "tabs", keys: () => ["Middle click"] },
  { id: "undo", group: "repo", keys: (m) => [`${mod(m)}+Z`] },
  {
    id: "redo",
    group: "repo",
    keys: (m) => (m ? ["⌘+Shift+Z"] : ["Ctrl+Shift+Z", "Ctrl+Y"]),
  },
  { id: "findCommit", group: "repo", keys: (m) => [`${mod(m)}+F`] },
  { id: "commit", group: "repo", keys: (m) => [`${mod(m)}+Enter`] },
  { id: "bindAccount", group: "repo", keys: (m) => [`${mod(m)}+${alt(m)}+1…9`] },
];

export const SHORTCUT_GROUPS: readonly ShortcutGroup[] = ["general", "tabs", "repo"];

/** `?` opens the cheat sheet, unless the key is meant for a text field, an editor or a dialog. */
export function isCheatSheetKey(e: KeyboardEvent): boolean {
  if (e.key !== "?" || e.ctrlKey || e.metaKey || e.altKey) return false;
  const target = e.target;
  if (target instanceof HTMLElement && target.isContentEditable) return false;
  return !(
    target instanceof Element &&
    target.closest('input, textarea, select, .monaco-editor, .xterm, [role="dialog"]')
  );
}
