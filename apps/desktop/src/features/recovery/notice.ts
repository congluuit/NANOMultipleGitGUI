import i18n from "i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { focusCommitSummary } from "@/features/command-palette/actions";
import { useLfsManager } from "@/features/lfs/managerStore";
import { useStashUi } from "@/features/stash/store";
import { invalidateRepo } from "@/features/worktree/queries";
import { commands, errorMessage, type RepoConfigFix } from "@/ipc";
import { useUi, type Notice, type NoticeAction } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { detectProblem, type Problem } from "./detect";
import { pullBeforePush } from "./pull";

/** A notice with a fix to click stays long enough to read it. */
export const RECOVERY_NOTICE_MS = 20_000;

function activeRepoId(): number | undefined {
  const { tabs, activeId } = useWorkspace.getState();
  const tab = tabs.find((x) => x.id === activeId);
  return tab && !tab.missing ? tab.id : undefined;
}

/**
 * Runs a fix, then the failed operation again when the notice knows how (`retry`); otherwise
 * says it is fixed so the user can try again.
 */
async function applyFix(
  fix: () => Promise<unknown>,
  notice: Notice,
  repoId: number | undefined,
  done: string,
) {
  try {
    await fix();
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
    return;
  } finally {
    if (repoId !== undefined) invalidateRepo(repoId);
  }
  if (notice.retry) {
    notice.retry();
  } else {
    useUi
      .getState()
      .showNotice({ kind: "success", message: done, hint: i18n.t("recovery.tryAgain"), repoId });
  }
}

function retryAction(notice: Notice): NoticeAction[] {
  return notice.retry ? [{ label: i18n.t("recovery.retry"), run: notice.retry }] : [];
}

function fixAction(
  label: string,
  notice: Notice,
  repoId: number | undefined,
  fix: () => Promise<unknown>,
  done: string,
): NoticeAction {
  return { label, run: () => void applyFix(fix, notice, repoId, done) };
}

function configFix(notice: Notice, repoId: number | undefined, fix: RepoConfigFix) {
  if (repoId === undefined) return [];
  const label = i18n.t(notice.retry ? `recovery.${fix}.fixRetry` : `recovery.${fix}.fix`);
  return [
    fixAction(
      label,
      notice,
      repoId,
      () => commands.repairConfig(repoId, fix),
      i18n.t(`recovery.${fix}.done`),
    ),
  ];
}

function problemActions(problem: Problem, notice: Notice, repoId: number | undefined) {
  const withRetry = Boolean(notice.retry);
  switch (problem.kind) {
    case "lock":
      if (repoId === undefined) return retryAction(notice);
      return [
        fixAction(
          i18n.t(withRetry ? "recovery.lock.fixRetry" : "recovery.lock.fix", {
            file: problem.file,
          }),
          notice,
          repoId,
          () => commands.repairRemoveLock(repoId, problem.path),
          i18n.t("recovery.lock.done", { file: problem.file }),
        ),
        ...retryAction(notice),
      ];
    case "indexCorrupt":
      if (repoId === undefined) return [];
      return [
        fixAction(
          i18n.t("recovery.indexCorrupt.fix"),
          notice,
          repoId,
          () => commands.repairRebuildIndex(repoId),
          i18n.t("recovery.indexCorrupt.done"),
        ),
      ];
    case "longPaths":
      return configFix(notice, repoId, "longPaths");
    case "postBuffer":
      return configFix(notice, repoId, "postBuffer");
    case "dubiousOwnership":
      return [
        fixAction(
          i18n.t("recovery.dubiousOwnership.fix"),
          notice,
          repoId,
          () => commands.repairTrustFolder(problem.path),
          i18n.t("recovery.dubiousOwnership.done"),
        ),
      ];
    case "unrelatedHistories":
      if (repoId === undefined) return [];
      return [
        {
          label: i18n.t("recovery.unrelatedHistories.fix"),
          run: () => void pullBeforePush(repoId, "mergeUnrelated"),
        },
      ];
    case "identityUnknown":
      return [
        {
          label: i18n.t("failureHelp.action.accounts"),
          run: () => useAccountDialogs.getState().openManager(),
        },
      ];
    case "noCommits":
      if (repoId === undefined) return [];
      return [
        {
          label: i18n.t("recovery.noCommits.fix"),
          run: () => void focusCommitSummary(repoId),
        },
      ];
    case "fileBusy":
      return retryAction(notice);
  }
}

/** Buttons for failures git already classified (`failureNotice`), next to its own ones. */
function reasonActions(notice: Notice, repoId: number | undefined): NoticeAction[] {
  if (repoId === undefined) return [];
  switch (notice.reason) {
    case "nonFastForward":
      return [
        { label: i18n.t("recovery.pullThenPush"), run: () => void pullBeforePush(repoId, "merge") },
      ];
    case "localChanges":
    case "untrackedFiles":
      return [
        {
          label: i18n.t("recovery.stashFirst"),
          run: () => useStashUi.getState().openCreate(repoId),
        },
      ];
    case "fileTooLarge":
      return [
        {
          label: i18n.t("recovery.openLfs"),
          run: () => useLfsManager.getState().open(repoId, "patterns"),
        },
      ];
    default:
      return [];
  }
}

/** Raw git output of a notice, without repeating the headline. */
function rawText(notice: Notice): string {
  return [notice.message, notice.detail]
    .filter((x): x is string => !!x && x.trim() !== "")
    .join("\n");
}

/**
 * Turns an error notice into one that explains the problem in plain words and offers the fix:
 * registered with `setNoticeEnhancer`, so every error shown in the notice bar goes through it.
 */
export function enhanceNotice(notice: Notice): Notice {
  if (notice.kind !== "error") return notice;
  const repoId = notice.repoId ?? activeRepoId();
  const problem = detectProblem(rawText(notice));
  if (problem) {
    const file = problem.kind === "lock" ? problem.file : "";
    return {
      ...notice,
      repoId,
      message: i18n.t(`recovery.${problem.kind}.title`, { file }),
      hint: i18n.t(`recovery.${problem.kind}.hint`, { file }),
      detail: rawText(notice),
      actions: [...problemActions(problem, notice, repoId), ...(notice.actions ?? [])],
      durationMs: RECOVERY_NOTICE_MS,
    };
  }
  const extra = reasonActions(notice, repoId);
  if (!extra.length) return notice;
  return {
    ...notice,
    repoId,
    actions: [...(notice.actions ?? []), ...extra],
    durationMs: RECOVERY_NOTICE_MS,
  };
}
