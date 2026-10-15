import { useEffect } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { useRepoRefs } from "@/features/repos/queries";
import { useWorktreeStatus } from "@/features/worktree/queries";
import { useUndoStore } from "./store";

/**
 * Undo / redo state of a repository. Re-read whenever its refs or working tree are re-read
 * (after any operation of the app, or a change seen by the file watcher), so operations made
 * in a terminal show up too.
 */
export function useUndoState(repoId: number | null) {
  const enabled = repoId !== null;
  const refsAt = useRepoRefs(repoId, enabled).dataUpdatedAt;
  const statusAt = useWorktreeStatus(repoId ?? -1, enabled).dataUpdatedAt;
  const query = useQuery({
    queryKey: ["undo", repoId, refsAt, statusAt],
    queryFn: () => commands.undoState(repoId!),
    enabled: isTauri() && enabled,
    placeholderData: keepPreviousData,
    gcTime: 5_000,
  });
  const setState = useUndoStore((s) => s.setState);
  useEffect(() => {
    if (repoId !== null && query.data) setState(repoId, query.data);
  }, [repoId, query.data, setState]);
  return query;
}
