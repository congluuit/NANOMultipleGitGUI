/** The text the editor's preview shows, kept up to date with the buffer as the user types. */

interface Disposable {
  dispose: () => void;
}

/** The part of a Monaco editor the preview follows. */
export interface TextSource {
  getModel: () => { getValue: () => string } | null;
  onDidChangeModelContent: (listener: () => void) => Disposable;
  onDidChangeModel: (listener: () => void) => Disposable;
}

/** Edits are taken this long after the last keystroke (a big file renders once per pause). */
export const PREVIEW_DELAY_MS = 150;

/**
 * A `useSyncExternalStore` source of the model's text: read when the preview opens, after
 * each pause in typing, and at once when the editor shows another model.
 */
export function modelTextStore(editor: TextSource | null, enabled: boolean) {
  if (!editor || !enabled) return { subscribe: () => () => {}, get: () => "" };
  let text = editor.getModel()?.getValue() ?? "";
  return {
    subscribe(notify: () => void) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const read = () => {
        clearTimeout(timer);
        text = editor.getModel()?.getValue() ?? "";
        notify();
      };
      const later = () => {
        clearTimeout(timer);
        timer = setTimeout(read, PREVIEW_DELAY_MS);
      };
      const subs = [editor.onDidChangeModelContent(later), editor.onDidChangeModel(read)];
      // The model may have been set between the first read and this subscription.
      read();
      return () => {
        clearTimeout(timer);
        subs.forEach((d) => d.dispose());
      };
    },
    get: () => text,
  };
}
