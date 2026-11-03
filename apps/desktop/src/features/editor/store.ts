/**
 * State of the built-in file editor that lives outside Monaco (which is loaded lazily): view
 * preferences kept on this computer, and which files have unsaved edits. The editor buffers
 * themselves (Monaco models, see buffers.ts) register how to save or drop them here, so the
 * diff header, the close guard and the shell can act on them without importing Monaco.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface EditorPrefs {
  /** Show the original text of changed lines between the lines (removed lines in red). */
  showRemoved: boolean;
  wordWrap: boolean;
  whitespace: boolean;
  minimap: boolean;
  /** Author and commit of the cursor line, after its end. */
  blame: boolean;
}

interface EditorPrefsState extends EditorPrefs {
  setPref: <K extends keyof EditorPrefs>(key: K, value: EditorPrefs[K]) => void;
}

export const useEditorPrefs = create<EditorPrefsState>()(
  persist(
    (set) => ({
      showRemoved: true,
      wordWrap: false,
      whitespace: false,
      minimap: true,
      blame: false,
      setPref: (key, value) => set({ [key]: value }),
    }),
    { name: "nanomgg.editorPrefs", version: 1 },
  ),
);

export const bufferKey = (repoId: number, path: string) => `${repoId}:${path}`;

interface DirtyState {
  /** Buffer keys with unsaved edits. */
  dirty: Record<string, true>;
  setDirty: (key: string, dirty: boolean) => void;
}

export const useDirtyBuffers = create<DirtyState>()((set) => ({
  dirty: {},
  setDirty: (key, dirty) =>
    set((s) => {
      if (!!s.dirty[key] === dirty) return s;
      const next = { ...s.dirty };
      if (dirty) next[key] = true;
      else delete next[key];
      return { dirty: next };
    }),
}));

export const useIsDirty = (repoId: number, path: string) =>
  useDirtyBuffers((s) => !!s.dirty[bufferKey(repoId, path)]);

export function dirtyCount(): number {
  return Object.keys(useDirtyBuffers.getState().dirty).length;
}

interface BufferHandle {
  /** Writes the buffer to disk; resolves to whether it was saved. */
  save: () => Promise<boolean>;
  /** Forgets the buffer and its unsaved edits. */
  drop: () => void;
}

const handles = new Map<string, BufferHandle>();

export function registerBuffer(key: string, handle: BufferHandle): () => void {
  handles.set(key, handle);
  return () => {
    if (handles.get(key) === handle) handles.delete(key);
  };
}

/** Saves the unsaved edits of a file, if any. `false` when there were edits and saving failed. */
export async function saveBuffer(repoId: number, path: string): Promise<boolean> {
  const key = bufferKey(repoId, path);
  if (!useDirtyBuffers.getState().dirty[key]) return true;
  return (await handles.get(key)?.save()) ?? true;
}

/** Throws away the unsaved edits of a file (before discarding it, for instance). */
export function dropBuffer(repoId: number, path: string) {
  const key = bufferKey(repoId, path);
  handles.get(key)?.drop();
  useDirtyBuffers.getState().setDirty(key, false);
}
