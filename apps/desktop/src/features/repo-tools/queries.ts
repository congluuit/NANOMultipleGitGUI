import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const toolKeys = {
  overview: () => ["workspaceOverview"] as const,
  /** Under "opstate" so every repository refresh re-reads it. */
  bisect: (id: number) => ["opstate", id, "bisect"] as const,
  reflog: (id: number) => ["reflog", id] as const,
  recentBranches: (id: number) => ["reflog", id, "recentBranches"] as const,
  stats: (id: number) => ["repoStats", id] as const,
  sparse: (id: number) => ["sparse", id] as const,
};

export function useWorkspaceOverview(enabled: boolean) {
  return useQuery({
    queryKey: toolKeys.overview(),
    queryFn: () => commands.workspaceOverview(),
    enabled: isTauri() && enabled,
    gcTime: 0,
  });
}

export const refreshOverview = () =>
  queryClient.invalidateQueries({ queryKey: toolKeys.overview() });

export function useBisectState(repoId: number, enabled = true) {
  return useQuery({
    queryKey: toolKeys.bisect(repoId),
    queryFn: () => commands.bisectState(repoId),
    enabled: isTauri() && enabled,
  });
}

export function useReflog(repoId: number, enabled: boolean) {
  return useQuery({
    queryKey: toolKeys.reflog(repoId),
    queryFn: () => commands.reflogList(repoId, 300),
    enabled: isTauri() && enabled,
    gcTime: 0,
  });
}

/** Branches checked out recently, read from git's own reflog. */
export function useReflogRecentBranches(repoId: number) {
  return useQuery({
    queryKey: toolKeys.recentBranches(repoId),
    queryFn: () => commands.reflogRecentBranches(repoId, 10),
    enabled: isTauri() && repoId >= 0,
    staleTime: 30_000,
  });
}

export function useRepoStats(repoId: number, enabled: boolean) {
  return useQuery({
    queryKey: toolKeys.stats(repoId),
    queryFn: () => commands.repoStats(repoId),
    enabled: isTauri() && enabled,
    staleTime: 60_000,
  });
}

export function useSparseState(repoId: number, enabled: boolean) {
  return useQuery({
    queryKey: toolKeys.sparse(repoId),
    queryFn: () => commands.sparseState(repoId),
    enabled: isTauri() && enabled,
    gcTime: 0,
  });
}
