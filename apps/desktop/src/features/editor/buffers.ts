/**
 * Editor buffers: one Monaco model per edited working-tree file, kept while it has unsaved
 * edits so switching to another file (or closing the view) loses nothing and keeps the undo
 * history. Clean buffers are disposed once no editor shows them. Components read a buffer
 * through `useSyncExternalStore` (`subscribeBuffers` / `bufferInfo`).
 *
 * A buffer also knows the encoding and line endings of its file, and saves it back the same
 * way unless the user picks another encoding or converts the line endings. A file that mixes
 * line endings cannot be kept as it is by Monaco (one line ending per model), so it stays
 * read-only until the user picks LF or CRLF.
 */
import i18n from "i18next";
import { invalidateWorktree } from "@/features/worktree/queries";
import { commands, errorMessage, type LineEnding, type TextEncoding } from "@/ipc";
import { useUi } from "@/stores/ui";
import { languageFor, monaco } from "@/features/diff/monaco";
import { encodingLabel, encodingOverride, useEncodingOverrides, type Eol } from "./encoding";
import { bufferKey, registerBuffer, useDirtyBuffers } from "./store";

/** A file as read from disk for a buffer. */
export interface DiskText {
  text: string;
  version: string;
  encoding: TextEncoding;
  eol: LineEnding;
}

interface Buffer {
  key: string;
  repoId: number;
  path: string;
  model: monaco.editor.ITextModel;
  /** Version on disk the buffer was loaded from or last saved as. */
  version: string;
  /** `getAlternativeVersionId()` at that point: equal again after undoing back to it. */
  savedAltId: number;
  /** Encoding of the file on disk. */
  diskEncoding: TextEncoding;
  /** Encoding the next save writes. */
  encoding: TextEncoding;
  /** The file mixes line endings and none was picked yet: read-only. */
  mixedEol: boolean;
  /** Line endings were picked for a mixed file: to save even if the text looks the same. */
  eolPending: boolean;
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
  /** Encoding the next save writes. */
  encoding: TextEncoding;
  /** Line endings are mixed and none was picked yet (the buffer is read-only). */
  mixedEol: boolean;
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
  return (
    b.model.getAlternativeVersionId() !== b.savedAltId ||
    b.encoding !== b.diskEncoding ||
    b.eolPending
  );
}

function changed(b: Buffer) {
  const dirty = isDirty(b);
  const i = b.info;
  if (
    i.dirty !== dirty ||
    i.conflict !== b.conflict ||
    i.encoding !== b.encoding ||
    i.mixedEol !== b.mixedEol
  ) {
    b.info = {
      model: b.model,
      dirty,
      conflict: b.conflict,
      encoding: b.encoding,
      mixedEol: b.mixedEol,
    };
    useDirtyBuffers.getState().setDirty(b.key, dirty);
    for (const l of listeners) l();
  }
}

/** Gives the model the file's line ending (Monaco guesses it, and uses the OS one for none). */
function applyEol(model: monaco.editor.ITextModel, eol: LineEnding) {
  if (eol === "lf") model.setEOL(monaco.editor.EndOfLineSequence.LF);
  if (eol === "crlf") model.setEOL(monaco.editor.EndOfLineSequence.CRLF);
}

/** The buffer of a file, created from what was read when there is none yet. */
export function acquireBuffer(repoId: number, path: string, file: DiskText) {
  const key = bufferKey(repoId, path);
  const existing = buffers.get(key);
  if (existing) {
    syncDisk(existing, file);
    return;
  }
  const model = monaco.editor.createModel(file.text, languageFor(path));
  applyEol(model, file.eol);
  const b: Buffer = {
    key,
    repoId,
    path,
    model,
    version: file.version,
    savedAltId: model.getAlternativeVersionId(),
    diskEncoding: file.encoding,
    encoding: file.encoding,
    mixedEol: file.eol === "mixed",
    eolPending: false,
    conflict: null,
    reloadNext: false,
    info: {
      model,
      dirty: false,
      conflict: null,
      encoding: file.encoding,
      mixedEol: file.eol === "mixed",
    },
    unregister: () => {},
  };
  model.onDidChangeContent(() => changed(b));
  b.unregister = registerBuffer(key, {
    save: () => save(b, false),
    drop: () => {
      if (b.model.isAttachedToEditor()) {
        b.savedAltId = b.model.getAlternativeVersionId();
        b.encoding = b.diskEncoding;
        b.eolPending = false;
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

/**
 * A (re)read of the file on disk: taken when the buffer is clean, a conflict otherwise. The
 * same bytes read in another encoding (reopened) count as a new read.
 */
function syncDisk(b: Buffer, file: DiskText) {
  const same = file.version === b.version && file.encoding === b.diskEncoding;
  if (same && !b.reloadNext) {
    b.conflict = null;
  } else if (isDirty(b) && !b.reloadNext) {
    b.conflict = file.version;
  } else {
    reload(b, file);
  }
  changed(b);
}

function reload(b: Buffer, file: DiskText) {
  if (b.model.getValue(monaco.editor.EndOfLinePreference.TextDefined, true) !== file.text) {
    b.model.setValue(file.text);
  }
  applyEol(b.model, file.eol);
  b.version = file.version;
  b.savedAltId = b.model.getAlternativeVersionId();
  b.diskEncoding = file.encoding;
  b.encoding = file.encoding;
  b.mixedEol = file.eol === "mixed";
  b.eolPending = false;
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
    const f = await commands.worktreeFileRead(b.repoId, b.path, encodingOverride(b.repoId, b.path));
    reload(b, {
      text: f.text ?? "",
      version: f.version,
      encoding: f.encoding ?? b.diskEncoding,
      eol: f.eol ?? "none",
    });
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

/** The line ending a save writes: the model's, or "as they are" while still mixed. */
function saveEol(b: Buffer): LineEnding {
  if (b.mixedEol) return "mixed";
  return b.model.getEOL() === "\r\n" ? "crlf" : "lf";
}

async function save(b: Buffer, overwrite: boolean): Promise<boolean> {
  // The BOM is the encoding's business (Monaco keeps a stray one apart: written back too).
  const text = b.model.getValue(monaco.editor.EndOfLinePreference.TextDefined, true);
  const altId = b.model.getAlternativeVersionId();
  const encoding = b.encoding;
  try {
    const out = await commands.worktreeFileWrite(
      b.repoId,
      b.path,
      text,
      encoding,
      saveEol(b),
      overwrite ? null : b.version,
    );
    if (out.unencodable) {
      const u = out.unencodable;
      useUi.getState().showNotice({
        kind: "error",
        message: i18n.t("editor.encoding.unencodable", {
          character: u.character,
          line: u.line,
          column: u.column,
          encoding: encodingLabel(encoding, i18n.t),
        }),
        repoId: b.repoId,
      });
      return false;
    }
    if (!out.saved) {
      b.conflict = out.version;
      changed(b);
      return false;
    }
    if (encoding !== b.diskEncoding) {
      // Read it back in the encoding it now has (a detection could guess another one).
      useEncodingOverrides.getState().set(b.repoId, b.path, encoding);
    }
    b.version = out.version;
    b.savedAltId = altId;
    b.diskEncoding = encoding;
    b.eolPending = false;
    b.conflict = null;
    changed(b);
    invalidateWorktree(b.repoId);
    return true;
  } catch (e) {
    notifyError(b.repoId, e);
    return false;
  }
}

/** "Save with encoding": writes the buffer in `encoding`; it keeps its encoding if that fails. */
export async function saveWithEncoding(key: string, encoding: TextEncoding): Promise<boolean> {
  const b = buffers.get(key);
  if (!b || b.mixedEol) return false;
  const before = b.encoding;
  b.encoding = encoding;
  changed(b);
  const ok = await save(b, false);
  if (!ok && b.encoding === encoding) {
    b.encoding = before;
    changed(b);
  }
  return ok;
}

/** Converts every line ending of the buffer (an undoable edit; saved with the next save). */
export function convertEol(key: string, eol: Eol) {
  const b = buffers.get(key);
  if (!b) return;
  const target =
    eol === "crlf" ? monaco.editor.EndOfLineSequence.CRLF : monaco.editor.EndOfLineSequence.LF;
  if (b.mixedEol) {
    // Monaco already normalized the text to one line ending: what is left is to save it.
    b.mixedEol = false;
    b.eolPending = true;
  }
  b.model.pushEOL(target);
  changed(b);
}
