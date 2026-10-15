import { create } from "zustand";
import type { IssueFilter } from "./queries";
import { DEFAULT_FILTER } from "./queries";

interface IssueUi {
  /** Repository whose "New issue" dialog is open. */
  createFor: number | null;
  /** Sidebar filter per repository. */
  filters: Record<number, IssueFilter>;
  openCreate: (repoId: number) => void;
  closeCreate: () => void;
  setFilter: (repoId: number, patch: Partial<IssueFilter>) => void;
}

export const useIssueUi = create<IssueUi>()((set) => ({
  createFor: null,
  filters: {},
  openCreate: (repoId) => set({ createFor: repoId }),
  closeCreate: () => set({ createFor: null }),
  setFilter: (repoId, patch) =>
    set((s) => ({
      filters: { ...s.filters, [repoId]: { ...(s.filters[repoId] ?? DEFAULT_FILTER), ...patch } },
    })),
}));

/** Filter of a repository's issue list (stable object: the default constant when unset). */
export const useIssueFilter = (repoId: number) =>
  useIssueUi((s) => s.filters[repoId] ?? DEFAULT_FILTER);
