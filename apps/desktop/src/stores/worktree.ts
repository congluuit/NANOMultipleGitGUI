import { create } from "zustand";
import type { DiffRequest } from "@/ipc";

export type DiffMode = "hunk" | "inline" | "split";

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
  /** Unsent commit messages survive switching tabs or opening a commit. */
  drafts: Record<number, CommitDraft>;
  openDiff: (repoId: number, request: DiffRequest | null) => void;
  setDiffMode: (mode: DiffMode) => void;
  setIgnoreWhitespace: (value: boolean) => void;
  updateDraft: (repoId: number, patch: Partial<CommitDraft>) => void;
  clearDraft: (repoId: number) => void;
}

export const useWorktreeStore = create<WorktreeStore>()((set) => ({
  diff: {},
  diffMode: "hunk",
  ignoreWhitespace: false,
  drafts: {},
  openDiff: (repoId, request) => set((s) => ({ diff: { ...s.diff, [repoId]: request } })),
  setDiffMode: (diffMode) => set({ diffMode }),
  setIgnoreWhitespace: (ignoreWhitespace) => set({ ignoreWhitespace }),
  updateDraft: (repoId, patch) =>
    set((s) => ({
      drafts: { ...s.drafts, [repoId]: { ...(s.drafts[repoId] ?? EMPTY_DRAFT), ...patch } },
    })),
  clearDraft: (repoId) => set((s) => ({ drafts: { ...s.drafts, [repoId]: EMPTY_DRAFT } })),
}));

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
