import { create } from "zustand";
import { useGraphStore } from "./graph";
import { useWorktreeStore } from "./worktree";

/** A file view shown in the center area instead of the graph / diff. */
export type FileView =
  | { kind: "conflict"; path: string }
  | { kind: "history"; path: string }
  /** `rev`: hex commit id or `HEAD`; `null` = working tree. */
  | { kind: "blame"; path: string; rev: string | null }
  | { kind: "file"; path: string; commit: string };

interface FileViewStore {
  views: Record<number, FileView | null>;
  open: (repoId: number, view: FileView | null) => void;
}

export const useFileViewStore = create<FileViewStore>()((set) => ({
  views: {},
  open: (repoId, view) => set((s) => ({ views: { ...s.views, [repoId]: view } })),
}));

export const useFileView = (repoId: number | null | undefined) =>
  useFileViewStore((s) => (repoId == null ? null : (s.views[repoId] ?? null)));

/** Opens a file view (closing the diff that would otherwise sit under it). */
export function openFileView(repoId: number, view: FileView) {
  useFileViewStore.getState().open(repoId, view);
  if (useWorktreeStore.getState().diff[repoId]) useWorktreeStore.getState().openDiff(repoId, null);
}

export function closeFileView(repoId: number) {
  useFileViewStore.getState().open(repoId, null);
}

/** Shows a commit in the graph: the graph is hidden while a file view is open. */
export function jumpToCommit(repoId: number, oid: string) {
  closeFileView(repoId);
  useWorktreeStore.getState().openDiff(repoId, null);
  useGraphStore.getState().requestJump(repoId, oid);
}

// Opening a diff (staging panel, commit details) or jumping in the graph replaces the view.
useWorktreeStore.subscribe((s, prev) => {
  for (const [id, req] of Object.entries(s.diff)) {
    const repoId = Number(id);
    if (req && req !== prev.diff[repoId] && useFileViewStore.getState().views[repoId]) {
      closeFileView(repoId);
    }
  }
});
useGraphStore.subscribe((s, prev) => {
  if (s.jump && s.jump !== prev.jump && useFileViewStore.getState().views[s.jump.repoId]) {
    closeFileView(s.jump.repoId);
  }
});
