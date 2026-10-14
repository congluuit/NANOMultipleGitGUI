interface Selectable {
  hasSelection(): boolean;
  getSelection(): string;
}

/**
 * xterm custom key handler (`false` = xterm ignores the key):
 * - Ctrl+Shift+C copies the selection (and must not open the WebView inspector);
 * - Ctrl+Shift+V is left to the browser, whose `paste` event xterm handles itself (bracketed
 *   paste included) without needing clipboard-read permission;
 * - Ctrl+` bubbles to the window shortcut that toggles the panel.
 */
export function terminalKeyHandler(
  term: Selectable,
  writeClipboard = (text: string) => navigator.clipboard.writeText(text),
) {
  return (e: KeyboardEvent): boolean => {
    if (e.ctrlKey && !e.altKey && e.code === "Backquote") return false;
    if (!e.ctrlKey || !e.shiftKey || e.altKey) return true;
    if (e.code === "KeyC") {
      if (e.type === "keydown" && term.hasSelection()) {
        void writeClipboard(term.getSelection()).catch(() => {});
      }
      e.preventDefault();
      return false;
    }
    if (e.code === "KeyV") return false;
    return true;
  };
}
