import { create } from "zustand";
import type { PullStateFilter } from "@/ipc";

/** Sidebar listing filters of one repository. */
export interface PullFilters {
  /** Remote whose project is listed; `null` = first hosted remote (`origin`). */
  remote: string | null;
  state: PullStateFilter;
  author: string;
  reviewer: string;
  label: string;
}

export const DEFAULT_FILTERS: PullFilters = {
  remote: null,
  state: "open",
  author: "",
  reviewer: "",
  label: "",
};

/** What the "Create pull request" dialog starts from. */
export interface CreatePullRequest {
  repoId: number;
  /** Source branch; `null` = the current branch. */
  branch: string | null;
}

interface PullUi {
  filters: Record<number, PullFilters>;
  create: CreatePullRequest | null;
  setFilters: (repoId: number, patch: Partial<PullFilters>) => void;
  openCreate: (request: CreatePullRequest) => void;
  closeCreate: () => void;
}

export const usePullUi = create<PullUi>()((set) => ({
  filters: {},
  create: null,
  setFilters: (repoId, patch) =>
    set((s) => ({
      filters: { ...s.filters, [repoId]: { ...(s.filters[repoId] ?? DEFAULT_FILTERS), ...patch } },
    })),
  openCreate: (create) => set({ create }),
  closeCreate: () => set({ create: null }),
}));

/** Filters of a repo (stable default object when none were set). */
export const usePullFilters = (repoId: number) =>
  usePullUi((s) => s.filters[repoId] ?? DEFAULT_FILTERS);

/** Opens the "Create pull / merge request" dialog. */
export const openCreatePull = (repoId: number, branch: string | null = null) =>
  usePullUi.getState().openCreate({ repoId, branch });
