/**
 * Editor buffers: one Monaco model per edited working-tree file, kept while it has unsaved
 * edits so switching to another file (or closing the view) loses nothing and keeps the undo
 * history. Clean buffers are disposed once no editor shows them. Components read a buffer
 * through `useSyncExternalStore` (`subscribeBuffers` / `bufferInfo`).
 */
import { invalidateWorktree } from "@/features/worktree/queries";
import { commands, errorMessage } from "@/ipc";
import { useUi } from "@/stores/ui";
import { languageFor, monaco } from "@/features/diff/monaco";
import { bufferKey, registerBuffer, useDirtyBuffers } from "./store";

interface Buffer {
  key: string;
  repoId: number;
  path: string;
  model: monaco.editor.ITextModel;
  /** Version on disk the buffer was loaded from or last saved as. */
  version: string;
  /** `getAlternativeVersionId()` at that point: equal again after undoing back to it. */
  savedAltId: number;
  /** Newer version on disk not taken because the buffer has unsaved edits. */
  conflict: string | null;
  /** Edits were thrown away while an editor showed the buffer: take the next disk read. */
  reloadNext: boolean;
  info: BufferInfo;
  unregister: () => void;
}

/** What the UI shows of a buffer; a new object on every change. */
export interface BufferInfo {
  model: monaco.editor.ITextModel;
  dirty: boolean;
  conflict: string | null;
}

const buffers = new Map<string, Buffer>();
const listeners = new Set<() => void>();

export function subscribeBuffers(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function bufferInfo(key: string): BufferInfo | null {
  return buffers.get(key)?.info ?? null;
}

function isDirty(b: Buffer): boolean {
  return b.model.getAlternativeVersionId() !== b.savedAltId;
}

function changed(b: Buffer) {
  const dirty = isDirty(b);
  if (b.info.dirty !== dirty || b.info.conflict !== b.conflict) {
    b.info = { model: b.model, dirty, conflict: b.conflict };
    useDirtyBuffers.getState().setDirty(b.key, dirty);
    for (const l of listeners) l();
  }
}

/** The buffer of a file, created from `text` (read at `version`) when there is none yet. */
export function acquireBuffer(repoId: number, path: string, text: string, version: string) {
  const key = bufferKey(repoId, path);
  const existing = buffers.get(key);
  if (existing) {
    syncDisk(existing, text, version);
    return;
  }
  const model = monaco.editor.createModel(text, languageFor(path));
  const b: Buffer = {
    key,
    repoId,
    path,
    model,
    version,
    savedAltId: model.getAlternativeVersionId(),
    conflict: null,
    reloadNext: false,
    info: { model, dirty: false, conflict: null },
    unregister: () => {},
  };
  model.onDidChangeContent(() => changed(b));
  b.unregister = registerBuffer(key, {
    save: () => save(b, false),
    drop: () => {
      if (b.model.isAttachedToEditor()) {
        b.savedAltId = b.model.getAlternativeVersionId();
        b.reloadNext = true;
        changed(b);
      } else {
        dispose(b);
      }
    },
  });
  buffers.set(key, b);
  for (const l of listeners) l();
}

/** An editor stopped showing the buffer: it goes away unless it has unsaved edits. */
export function releaseBuffer(key: string) {
  const b = buffers.get(key);
  if (b && !b.model.isAttachedToEditor() && !isDirty(b)) dispose(b);
}

function dispose(b: Buffer) {
  buffers.delete(b.key);
  b.unregister();
  b.model.dispose();
  useDirtyBuffers.getState().setDirty(b.key, false);
  for (const l of listeners) l();
}

/** A (re)read of the file on disk: taken when the buffer is clean, a conflict otherwise. */
function syncDisk(b: Buffer, text: string, version: string) {
  if (version === b.version && !b.reloadNext) {
    b.conflict = null;
  } else if (isDirty(b) && !b.reloadNext) {
    b.conflict = version;
  } else {
    reload(b, text, version);
  }
  changed(b);
}

function reload(b: Buffer, text: string, version: string) {
  if (b.model.getValue(monaco.editor.EndOfLinePreference.TextDefined, true) !== text) {
    b.model.setValue(text);
  }
  b.version = version;
  b.savedAltId = b.model.getAlternativeVersionId();
  b.conflict = null;
  b.reloadNext = false;
  changed(b);
}

function notifyError(repoId: number, e: unknown) {
  useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
}

/** Reads the file again and replaces the buffer with it, dropping unsaved edits. */
export async function reloadFromDisk(key: string): Promise<void> {
  const b = buffers.get(key);
  if (!b) return;
  try {
    const f = await commands.worktreeFileRead(b.repoId, b.path);
    reload(b, f.text ?? "", f.version);
  } catch (e) {
    notifyError(b.repoId, e);
  }
}

/**
 * Writes the buffer. Refused (returns `false`, sets `conflict`) when the file changed on disk
 * since it was loaded, unless `overwrite`.
 */
export function saveBuffer(key: string, overwrite = false): Promise<boolean> {
  const b = buffers.get(key);
  return b ? save(b, overwrite) : Promise.resolve(true);
}

async function save(b: Buffer, overwrite: boolean): Promise<boolean> {
  // Keep the BOM and the buffer's line breaks as they were read.
  const text = b.model.getValue(monaco.editor.EndOfLinePreference.TextDefined, true);
  const altId = b.model.getAlternativeVersionId();
  try {
    const out = await commands.worktreeFileWrite(
      b.repoId,
      b.path,
      text,
      overwrite ? null : b.version,
    );
    if (!out.saved) {
      b.conflict = out.version;
      changed(b);
      return false;
    }
    b.version = out.version;
    b.savedAltId = altId;
    b.conflict = null;
    changed(b);
    invalidateWorktree(b.repoId);
    return true;
  } catch (e) {
    notifyError(b.repoId, e);
    return false;
  }
}
