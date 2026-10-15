import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const lfsKeys = {
  status: (id: number) => ["lfs", id] as const,
  locks: (id: number) => ["lfsLocks", id] as const,
  files: (id: number) => ["lfsFiles", id] as const,
  storage: (id: number) => ["lfsStorage", id] as const,
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
  void queryClient.invalidateQueries({ queryKey: lfsKeys.files(repoId) });
}

/**
 * LFS locks of the repository (network: the lock API, as the repo's account). Only for repos
 * that use LFS; kept for 2 minutes and never retried (servers without lock support fail).
 */
export function useLfsLocks(repoId: number | null | undefined, enabled = true) {
  return useQuery({
    queryKey: lfsKeys.locks(repoId ?? -1),
    queryFn: () => commands.lfsLocks(repoId!),
    enabled: isTauri() && repoId != null && enabled,
    staleTime: 120_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
}

export function invalidateLfsLocks(repoId: number) {
  void queryClient.invalidateQueries({ queryKey: lfsKeys.locks(repoId) });
}

/** LFS files of the checkout (size, downloaded / pointer, lockable). */
export function useLfsFiles(repoId: number, enabled = true) {
  return useQuery({
    queryKey: lfsKeys.files(repoId),
    queryFn: () => commands.lfsFiles(repoId),
    enabled: isTauri() && enabled,
    staleTime: 10_000,
  });
}

/** Size of the local LFS object store. */
export function useLfsStorage(repoId: number, enabled = true) {
  return useQuery({
    queryKey: lfsKeys.storage(repoId),
    queryFn: () => commands.lfsStorage(repoId),
    enabled: isTauri() && enabled,
    staleTime: 30_000,
  });
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
