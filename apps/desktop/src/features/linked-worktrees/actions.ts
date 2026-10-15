import i18n from "i18next";
import { commands, type Outcome, type WorktreeTarget } from "@/ipc";
import { runOutcome } from "@/features/commits/outcome";
import { openRepository } from "@/features/repos/actions";
import { useUi } from "@/stores/ui";
import { invalidateLinkedWorktrees } from "./queries";
import { folderName } from "./model";

export async function addWorktree(
  repoId: number,
  path: string,
  target: WorktreeTarget,
  openAfter: boolean,
): Promise<boolean> {
  const w = await runOutcome(
    repoId,
    () => commands.linkedWorktreeAdd(repoId, path, target),
    (v) => i18n.t("worktrees.added", { folder: folderName(v.path) }),
  );
  invalidateLinkedWorktrees(repoId);
  if (w && openAfter) await openRepository(w.path);
  return w !== undefined;
}

/** Removes a worktree; `"dirty"` = it has changes and `force` was not set (ask first). */
export async function removeWorktree(
  repoId: number,
  path: string,
  force: boolean,
): Promise<"done" | "dirty" | "failed"> {
  let dirty = false;
  const value = await runOutcome(
    repoId,
    async (): Promise<Outcome<null>> => {
      const out = await commands.linkedWorktreeRemove(repoId, path, force);
      if (out.status === "rejected" && out.reason === "dirtyWorktree" && !force) {
        dirty = true;
        return { status: "done", value: null };
      }
      return out;
    },
    () => (dirty ? null : i18n.t("worktrees.removed", { folder: folderName(path) })),
  );
  invalidateLinkedWorktrees(repoId);
  if (dirty) return "dirty";
  return value === undefined ? "failed" : "done";
}

export async function setWorktreeLocked(
  repoId: number,
  path: string,
  locked: boolean,
  reason: string | null,
): Promise<void> {
  await runOutcome(
    repoId,
    () => commands.linkedWorktreeLock(repoId, path, locked, reason),
    () => i18n.t(locked ? "worktrees.locked" : "worktrees.unlocked", { folder: folderName(path) }),
  );
  invalidateLinkedWorktrees(repoId);
}

/** Lists what `git worktree prune` would drop; the caller confirms, then `pruneWorktrees`. */
export async function previewPrune(repoId: number): Promise<string[] | undefined> {
  return runOutcome(repoId, () => commands.linkedWorktreePrune(repoId, true));
}

export async function pruneWorktrees(repoId: number): Promise<void> {
  await runOutcome(
    repoId,
    () => commands.linkedWorktreePrune(repoId, false),
    (lines) => i18n.t("worktrees.pruned", { count: lines.length }),
  );
  invalidateLinkedWorktrees(repoId);
}

export function notifyNothingToPrune() {
  useUi.getState().showNotice({ kind: "info", message: i18n.t("worktrees.nothingToPrune") });
}
