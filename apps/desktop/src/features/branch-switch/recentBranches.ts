/**
 * Branches switched to recently, per repository (newest first), so the palette can offer
 * "Switch to previous branch" (like `git checkout -`) and a short "Recent branches" list.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";

export const RECENT_BRANCHES_LIMIT = 10;

interface RecentBranchesState {
  byRepo: Record<string, string[]>;
  record: (repoId: number, from: string | null, to: string) => void;
}

/** `to` first, then `from`, then the older ones (no duplicates, capped). */
export function pushRecent(list: readonly string[], from: string | null, to: string): string[] {
  const head = from && from !== to ? [to, from] : [to];
  return [...head, ...list.filter((b) => !head.includes(b))].slice(0, RECENT_BRANCHES_LIMIT);
}

/** Recent branches one can switch to now: not the current one, and still existing locally. */
export function recentChoices(
  list: readonly string[],
  current: string | null,
  localNames: readonly string[],
): string[] {
  const exists = new Set(localNames);
  return list.filter((b) => b !== current && exists.has(b));
}

export const useRecentBranches = create<RecentBranchesState>()(
  persist(
    (set) => ({
      byRepo: {},
      record: (repoId, from, to) =>
        set((s) => ({
          byRepo: { ...s.byRepo, [repoId]: pushRecent(s.byRepo[repoId] ?? [], from, to) },
        })),
    }),
    { name: "nanomgg.recentBranches", version: 1 },
  ),
);

export function recentBranchesOf(repoId: number): string[] {
  return useRecentBranches.getState().byRepo[repoId] ?? [];
}
