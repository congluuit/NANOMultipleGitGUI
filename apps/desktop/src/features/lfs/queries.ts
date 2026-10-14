import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const lfsKeys = {
  status: (id: number) => ["lfs", id] as const,
};

export function useLfsStatus(repoId: number, enabled = true) {
  return useQuery({
    queryKey: lfsKeys.status(repoId),
    queryFn: () => commands.lfsStatus(repoId),
    enabled: isTauri() && enabled,
    staleTime: 10_000,
  });
}

export function invalidateLfs(repoId: number) {
  void queryClient.invalidateQueries({ queryKey: lfsKeys.status(repoId) });
}

/**
 * Re-reads the LFS status of `repoId` when the watcher reports changes that may touch
 * `.gitattributes`, the LFS config or hooks: checkouts/config (refs) and staging (index).
 * Plain working-tree edits are ignored to avoid spawning git-lfs on every save.
 */
export function useLfsChangeEvents(repoId: number) {
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void events.repoChanged
      .listen(({ payload }) => {
        if (payload.repoId === repoId && (payload.refs || payload.index)) invalidateLfs(repoId);
      })
      .then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [repoId]);
}
