import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const linkedWorktreeKeys = {
  list: (id: number) => ["linkedWorktrees", id] as const,
};

export function useLinkedWorktrees(repoId: number, enabled = true) {
  return useQuery({
    queryKey: linkedWorktreeKeys.list(repoId),
    queryFn: () => commands.linkedWorktreeList(repoId),
    enabled: isTauri() && enabled,
    staleTime: 10_000,
  });
}

export function invalidateLinkedWorktrees(repoId: number) {
  void queryClient.invalidateQueries({ queryKey: linkedWorktreeKeys.list(repoId) });
}

/** Branch checkouts change which branch each worktree has: re-read on ref changes. */
export function useLinkedWorktreeEvents(repoId: number) {
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void events.repoChanged
      .listen(({ payload }) => {
        if (payload.repoId === repoId && payload.refs) invalidateLinkedWorktrees(repoId);
      })
      .then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [repoId]);
}
