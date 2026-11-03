import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { DiffRequest } from "@/ipc";

/** `file`: the whole file in the built-in editor (editable for working-tree files). */
export type DiffMode = "hunk" | "inline" | "split" | "file";

export interface CommitDraft {
  summary: string;
  body: string;
  amend: boolean;
  signoff: boolean;
  /** Message copied from HEAD when amend mode was entered; cleared again when leaving it. */
  prefill: string | null;
}

export const EMPTY_DRAFT: CommitDraft = {
  summary: "",
  body: "",
  amend: false,
  signoff: false,
  prefill: null,
};

interface WorktreeStore {
  /** File whose diff is shown in the center area, per repository. */
  diff: Record<number, DiffRequest | null>;
  diffMode: DiffMode;
  ignoreWhitespace: boolean;
  /** Inline / side by side: show every line instead of folding the unchanged ones. */
  wholeFile: boolean;
  /** Unsent commit messages survive switching tabs or opening a commit. */
  drafts: Record<number, CommitDraft>;
  openDiff: (repoId: number, request: DiffRequest | null) => void;
  setDiffMode: (mode: DiffMode) => void;
  setIgnoreWhitespace: (value: boolean) => void;
  setWholeFile: (value: boolean) => void;
  updateDraft: (repoId: number, patch: Partial<CommitDraft>) => void;
  clearDraft: (repoId: number) => void;
}

export const useWorktreeStore = create<WorktreeStore>()(
  persist(
    (set) => ({
      diff: {},
      diffMode: "hunk",
      ignoreWhitespace: false,
      wholeFile: false,
      drafts: {},
      openDiff: (repoId, request) => set((s) => ({ diff: { ...s.diff, [repoId]: request } })),
      setDiffMode: (diffMode) => set({ diffMode }),
      setIgnoreWhitespace: (ignoreWhitespace) => set({ ignoreWhitespace }),
      setWholeFile: (wholeFile) => set({ wholeFile }),
      updateDraft: (repoId, patch) =>
        set((s) => ({
          drafts: { ...s.drafts, [repoId]: { ...(s.drafts[repoId] ?? EMPTY_DRAFT), ...patch } },
        })),
      clearDraft: (repoId) => set((s) => ({ drafts: { ...s.drafts, [repoId]: EMPTY_DRAFT } })),
    }),
    {
      // Only the view choices are remembered on this computer.
      name: "nanomgg.diffView",
      version: 1,
      partialize: (s) => ({
        diffMode: s.diffMode,
        ignoreWhitespace: s.ignoreWhitespace,
        wholeFile: s.wholeFile,
      }),
    },
  ),
);

export const useOpenDiff = (repoId: number | null | undefined) =>
  useWorktreeStore((s) => (repoId == null ? null : (s.diff[repoId] ?? null)));

export const useDraft = (repoId: number) =>
  useWorktreeStore((s) => s.drafts[repoId] ?? EMPTY_DRAFT);

/** Two requests point at the same file view. */
export function sameDiff(a: DiffRequest | null, b: DiffRequest | null): boolean {
  return (
    !!a &&
    !!b &&
    a.side === b.side &&
    a.path === b.path &&
    (a.commit ?? null) === (b.commit ?? null)
  );
}
