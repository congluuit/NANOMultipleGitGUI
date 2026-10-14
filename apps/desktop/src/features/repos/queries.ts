import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const repoKeys = {
  refs: (id: number) => ["refs", id] as const,
  graph: (id: number) => ["graph", id] as const,
  recent: ["recent"] as const,
  tools: ["tools"] as const,
};

export function useToolStatus() {
  return useQuery({
    queryKey: repoKeys.tools,
    queryFn: () => commands.toolStatus(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}

export function useRepoRefs(id: number | null, available = true) {
  return useQuery({
    queryKey: repoKeys.refs(id ?? -1),
    queryFn: () => commands.repoRefs(id!),
    enabled: isTauri() && id !== null && available,
  });
}

export function useRecentRepos() {
  return useQuery({
    queryKey: repoKeys.recent,
    queryFn: () => commands.recentRepos(),
    enabled: isTauri(),
  });
}

/** Refreshes repository data when the backend watcher reports on-disk changes. */
export function useRepoChangeEvents() {
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void events.repoChanged
      .listen(({ payload }) => {
        const id = payload.repoId;
        if (payload.refs) {
          void queryClient.invalidateQueries({ queryKey: repoKeys.refs(id) });
          void queryClient.invalidateQueries({ queryKey: repoKeys.graph(id) });
        }
        // Status, diffs and editor contents (keys from features/worktree/queries.ts).
        if (payload.refs || payload.index || payload.worktree) {
          void queryClient.invalidateQueries({ queryKey: ["status", id] });
          void queryClient.invalidateQueries({ queryKey: ["diff", id] });
          void queryClient.invalidateQueries({ queryKey: ["contents", id] });
          void queryClient.invalidateQueries({ queryKey: ["opstate", id] });
        }
      })
      .then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}
