import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const submoduleKeys = {
  list: (id: number) => ["submodules", id] as const,
};

export function useSubmodules(repoId: number, enabled = true) {
  return useQuery({
    queryKey: submoduleKeys.list(repoId),
    queryFn: () => commands.submoduleList(repoId),
    enabled: isTauri() && enabled,
    staleTime: 10_000,
  });
}

export function invalidateSubmodules(repoId: number) {
  void queryClient.invalidateQueries({ queryKey: submoduleKeys.list(repoId) });
}

/** Re-reads the list when refs / index / working tree change (gitlinks, checkouts). */
export function useSubmoduleChangeEvents(repoId: number) {
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void events.repoChanged
      .listen(({ payload }) => {
        if (payload.repoId === repoId) invalidateSubmodules(repoId);
      })
      .then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [repoId]);
}
