/** Phase 6.1: what a branch switch has to do first (pure logic, tested on its own). */
import type { BranchSwitchCheck, BranchSwitchPrefs, SwitchDirtyMode, SwitchTarget } from "@/ipc";

export type SwitchPlan =
  /** Already on that branch. */
  | { kind: "same" }
  /** Merge / rebase / cherry-pick / revert stopped half-way, or files still in conflict. */
  | { kind: "operation" }
  /** A fetch / pull / push of this repository is running. */
  | { kind: "transfer" }
  /** The branch is checked out in another worktree. */
  | { kind: "worktree"; path: string }
  /** Detached HEAD on commits no branch keeps: warn first. */
  | { kind: "detached" }
  /** Uncommitted changes and the user wants to be asked. */
  | { kind: "dirty" }
  | { kind: "go"; mode: SwitchDirtyMode };

export const DEFAULT_PREFS: BranchSwitchPrefs = { dirtyAction: "ask", autoRestore: true };

export interface PlanOptions {
  /** A fetch / pull / push of the repository is running. */
  transferRunning: boolean;
  /** "Switch anyway" was chosen in the detached-HEAD warning. */
  detachedConfirmed?: boolean;
}

export function planSwitch(
  check: BranchSwitchCheck,
  prefs: BranchSwitchPrefs,
  { transferRunning, detachedConfirmed = false }: PlanOptions,
): SwitchPlan {
  if (check.alreadyCurrent) return { kind: "same" };
  if (check.operation || check.conflicts > 0) return { kind: "operation" };
  if (transferRunning) return { kind: "transfer" };
  if (check.checkedOutAt) return { kind: "worktree", path: check.checkedOutAt };
  if (check.detachedOrphan && !detachedConfirmed) return { kind: "detached" };
  if (check.changes === 0) return { kind: "go", mode: "stash" };
  if (prefs.dirtyAction === "ask") return { kind: "dirty" };
  return { kind: "go", mode: prefs.dirtyAction };
}

/** `main` / `origin/feature` as shown to the user. */
export function targetLabel(target: SwitchTarget): string {
  return target.kind === "local" ? target.name : `${target.remote}/${target.branch}`;
}

/** Last path segment of a worktree folder (`C:/src/app-wt` → `app-wt`). */
export function folderOf(path: string): string {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
