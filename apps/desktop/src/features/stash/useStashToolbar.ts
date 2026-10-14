import { useState } from "react";
import { useRepoRefs } from "@/features/repos/queries";
import { useWorktreeStatus } from "@/features/worktree/queries";
import type { Stash, WorkingTreeStatus } from "@/ipc";
import { useActiveRepo } from "@/stores/workspace";
import { applyStash } from "./actions";
import { useStashUi } from "./store";

/** The working tree has something `git stash` could save. */
export function hasLocalChanges(status: WorkingTreeStatus | undefined): boolean {
  return !!status && status.staged.length + status.unstaged.length + status.conflicted.length > 0;
}

/** `stash@{0}`, the entry the toolbar "Pop" button pops. */
export function topStash(stashes: Stash[] | undefined): Stash | null {
  return stashes?.find((s) => s.index === 0) ?? null;
}

/** State and handlers of the toolbar Stash / Pop buttons for the active repository. */
export function useStashToolbar() {
  const repo = useActiveRepo();
  const available = !!repo && !repo.missing;
  const repoId = repo?.id ?? null;
  const status = useWorktreeStatus(repoId ?? -1, available);
  const refs = useRepoRefs(repoId, available);
  const openCreate = useStashUi((s) => s.openCreate);
  const [popping, setPopping] = useState(false);
  const top = topStash(refs.data?.stashes);

  return {
    top,
    canStash: available && hasLocalChanges(status.data),
    canPop: available && top !== null && !popping,
    stash: () => {
      if (repoId !== null) openCreate(repoId);
    },
    pop: async () => {
      if (repoId === null || !top) return;
      setPopping(true);
      try {
        await applyStash(repoId, top.index, true);
      } finally {
        setPopping(false);
      }
    },
  };
}
