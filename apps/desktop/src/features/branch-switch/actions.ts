/**
 * Phase 6.1: the "safe branch switch" every entry point goes through (double-click in the
 * sidebar, Enter on a focused branch, Checkout in the branch menus and the palette).
 *
 * check (read-only) → explain what blocks it / ask about uncommitted changes → run (stash →
 * checkout → roll back on failure → restore the autostash of the branch we arrive on).
 */
import i18n from "i18next";
import { failureNotice, runOutcome } from "@/features/commits/outcome";
import { useRefDialog } from "@/features/commits/store";
import { openRepository } from "@/features/repos/actions";
import { updateSubmodules } from "@/features/submodules/actions";
import { selectRepoRunning, useTransferQueue } from "@/features/transfers/store";
import { invalidateRepo } from "@/features/worktree/queries";
import {
  commands,
  errorMessage,
  type AutostashRestore,
  type BranchSwitchCheck,
  type BranchSwitchResult,
  type SwitchDirtyMode,
  type SwitchTarget,
} from "@/ipc";
import { openFileView } from "@/stores/fileView";
import { useUi, type Notice, type NoticeAction } from "@/stores/ui";
import { folderOf, planSwitch, targetLabel } from "./model";
import { getPrefs } from "./prefs";
import { useBranchSwitchUi } from "./store";

/** Repositories with a switch in flight (a double-click must not start two). */
const busy = new Set<number>();

function notify(notice: Notice) {
  useUi.getState().showNotice(notice);
}

/** Entry point: switch `repoId` to `target` safely. */
export async function switchBranch(
  repoId: number,
  target: SwitchTarget,
  opts: { detachedConfirmed?: boolean } = {},
): Promise<void> {
  if (busy.has(repoId)) return;
  busy.add(repoId);
  let check: BranchSwitchCheck;
  try {
    check = await commands.branchSwitchCheck(repoId, target);
  } catch (e) {
    notify({ kind: "error", message: errorMessage(e) });
    busy.delete(repoId);
    return;
  }
  const prefs = await getPrefs();
  const transferRunning = selectRepoRunning(useTransferQueue.getState(), repoId) !== null;
  const plan = planSwitch(check, prefs, {
    transferRunning,
    detachedConfirmed: opts.detachedConfirmed,
  });
  busy.delete(repoId);

  switch (plan.kind) {
    case "same":
      notify({ kind: "info", message: i18n.t("branchSwitch.alreadyOn", { branch: check.branch }) });
      return;
    case "operation":
      notify(operationNotice(repoId, check.operation, check.conflicts));
      return;
    case "transfer":
      notify({
        kind: "info",
        message: i18n.t("branchSwitch.transferRunning"),
        hint: i18n.t("branchSwitch.transferRunningHint"),
      });
      return;
    case "worktree":
      notify(worktreeNotice(check.branch, plan.path));
      return;
    case "detached":
    case "dirty":
      useBranchSwitchUi.getState().ask({ kind: plan.kind, repoId, target, check });
      return;
    case "go":
      await runSwitch(repoId, target, plan.mode);
      return;
  }
}

/** Runs the switch with an answer to "what about the uncommitted changes". */
export async function runSwitch(
  repoId: number,
  target: SwitchTarget,
  mode: SwitchDirtyMode,
): Promise<void> {
  if (busy.has(repoId)) return;
  busy.add(repoId);
  try {
    const result = await commands.branchSwitchRun(repoId, target, mode);
    notify(resultNotice(repoId, target, mode, result));
  } catch (e) {
    notify({ kind: "error", message: errorMessage(e) });
  } finally {
    busy.delete(repoId);
    invalidateRepo(repoId);
  }
}

/** "Restore now": pops the autostash of the checked-out `branch`. */
export async function restoreAutostash(repoId: number, branch: string): Promise<void> {
  try {
    const r = await commands.branchSwitchRestore(repoId, branch);
    notify(
      r
        ? restoreNotice(repoId, branch, r)
        : { kind: "info", message: i18n.t("branchSwitch.restore.none", { branch }) },
    );
  } catch (e) {
    notify({ kind: "error", message: errorMessage(e) });
  } finally {
    invalidateRepo(repoId);
  }
}

/** "Create a branch here first" in the detached-HEAD warning. */
export function createBranchHere(repoId: number, check: BranchSwitchCheck) {
  const start = check.headShort ?? "HEAD";
  useRefDialog.getState().open({ kind: "createBranch", repoId, start, label: start });
}

// ---------------------------------------------------------------------------------------------
// Notices.

function operationNotice(
  repoId: number,
  operation: BranchSwitchCheck["operation"],
  conflicts: number,
): Notice {
  const op = operation ? i18n.t(`ops.kind.${operation}`) : null;
  const actions: NoticeAction[] = [];
  if (operation) {
    actions.push({
      label: i18n.t("branchSwitch.op.abort", { op }),
      run: () =>
        void runOutcome(
          repoId,
          () => commands.operationAbort(repoId),
          (o) => i18n.t("ops.aborted", { op: i18n.t(`ops.kind.${o}`) }),
        ),
    });
  }
  if (conflicts > 0 || !operation) actions.push(openConflictsAction(repoId));
  return {
    kind: "error",
    message: op
      ? i18n.t("branchSwitch.op.inProgress", { op })
      : conflicts > 0
        ? i18n.t("branchSwitch.op.conflicts", { count: conflicts })
        : i18n.t("branchSwitch.op.unfinished"),
    hint: op
      ? i18n.t("branchSwitch.op.inProgressHint", { op })
      : i18n.t("branchSwitch.op.conflictsHint"),
    actions,
  };
}

/** Opens the first conflicted file in the conflict editor. */
function openConflictsAction(repoId: number, paths?: string[]): NoticeAction {
  return {
    label: i18n.t("branchSwitch.openConflicts"),
    run: () =>
      void (async () => {
        const first =
          paths?.[0] ??
          (await commands
            .worktreeStatus(repoId)
            .then((s) => s.conflicted[0]?.path)
            .catch(() => undefined));
        if (first) openFileView(repoId, { kind: "conflict", path: first });
      })(),
  };
}

function worktreeNotice(branch: string, path: string): Notice {
  return {
    kind: "info",
    message: i18n.t("branchSwitch.worktree.title", { branch, folder: folderOf(path) }),
    hint: i18n.t("branchSwitch.worktree.hint", { branch, path }),
    actions: [
      { label: i18n.t("branchSwitch.worktree.open"), run: () => void openRepository(path) },
    ],
  };
}

function restoreNotice(repoId: number, branch: string, r: AutostashRestore): Notice {
  switch (r.status) {
    case "restored":
      return {
        kind: "success",
        message: i18n.t("branchSwitch.restore.done", { count: r.files, branch }),
      };
    case "conflicts":
      return {
        kind: "error",
        message: i18n.t("branchSwitch.restore.conflicts", { count: r.conflicts.length, branch }),
        hint: i18n.t("branchSwitch.restore.conflictsHint"),
        actions: [openConflictsAction(repoId, r.conflicts)],
      };
    case "pending":
      return {
        kind: "info",
        message: i18n.t("branchSwitch.restore.pending", { count: r.files, branch }),
        hint: i18n.t(
          r.dirty ? "branchSwitch.restore.pendingDirty" : "branchSwitch.restore.pendingOff",
        ),
        actions: [restoreAction(repoId, branch)],
      };
    case "failed":
      return {
        kind: "error",
        message: i18n.t("branchSwitch.restore.failed", { branch }),
        hint: i18n.t("branchSwitch.restore.failedHint", { branch }),
        detail: r.message ?? undefined,
      };
  }
}

function restoreAction(repoId: number, branch: string): NoticeAction {
  return {
    label: i18n.t("branchSwitch.restore.now"),
    run: () => void restoreAutostash(repoId, branch),
  };
}

function resultNotice(
  repoId: number,
  target: SwitchTarget,
  mode: SwitchDirtyMode,
  r: BranchSwitchResult,
): Notice {
  const branch = r.branch;
  if (r.status === "unchanged") {
    return { kind: "info", message: i18n.t("branchSwitch.alreadyOn", { branch }) };
  }
  if (r.status === "blocked") return blockedNotice(repoId, target, mode, r);

  const lines: string[] = [];
  const actions: NoticeAction[] = [];
  let kind: Notice["kind"] = "success";
  if (r.stashed) {
    lines.push(
      r.from
        ? i18n.t("branchSwitch.done.stashed", { count: r.changes, from: r.from })
        : i18n.t("branchSwitch.done.stashedDetached", { count: r.changes }),
    );
  }
  if (r.restore) {
    const sub = restoreNotice(repoId, branch, r.restore);
    lines.push(sub.message);
    if (sub.hint) lines.push(sub.hint);
    actions.push(...(sub.actions ?? []));
    if (sub.kind === "error") kind = "error";
    else if (sub.kind === "info") kind = "info";
  }
  if (r.submodules.length > 0) {
    lines.push(i18n.t("branchSwitch.done.submodules", { count: r.submodules.length }));
    actions.push({
      label: i18n.t("branchSwitch.done.updateSubmodules"),
      run: () => void updateSubmodules(repoId, r.submodules, false),
    });
    if (kind === "success") kind = "info";
  }
  if (r.from) {
    const from = r.from;
    actions.push({
      label: i18n.t("branchSwitch.done.back", { branch: from }),
      run: () => void switchBranch(repoId, { kind: "local", name: from }),
    });
  }
  return {
    kind,
    message: r.created
      ? i18n.t("branchSwitch.done.created", { branch, remote: targetLabel(target) })
      : i18n.t("branchSwitch.done.switched", { branch }),
    hint: lines.length ? lines.join("\n") : undefined,
    actions,
  };
}

function blockedNotice(
  repoId: number,
  target: SwitchTarget,
  mode: SwitchDirtyMode,
  r: BranchSwitchResult,
): Notice {
  const branch = r.branch;
  const message = r.message ?? "";
  switch (r.reason) {
    case "checkedOutElsewhere":
      return worktreeNotice(branch, r.worktree ?? "");
    case "operationInProgress":
    case "unmergedFiles":
      return operationNotice(repoId, null, 0);
    case "localChanges":
    case "untrackedFiles":
      if (mode === "carry") {
        return {
          kind: "error",
          message: i18n.t("branchSwitch.carry.refused", { branch }),
          hint: i18n.t("branchSwitch.carry.refusedHint", { branch }),
          detail: message || undefined,
          actions: [
            {
              label: i18n.t("branchSwitch.dialog.stash"),
              run: () => void runSwitch(repoId, target, "stash"),
            },
          ],
        };
      }
      break;
    default:
      break;
  }
  const base = r.reason
    ? failureNotice(r.reason, message, repoId)
    : { kind: "error" as const, message: message || i18n.t("branchSwitch.failed", { branch }) };
  return {
    ...base,
    message: i18n.t("branchSwitch.failed", { branch }) + " " + base.message,
    hint: [
      base.hint,
      r.stashKept ? i18n.t("branchSwitch.stashKept") : i18n.t("branchSwitch.nothingChanged"),
    ]
      .filter(Boolean)
      .join("\n"),
  };
}
