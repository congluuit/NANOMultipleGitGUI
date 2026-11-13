import i18n from "i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { accountKeys } from "@/features/accounts/queries";
import { bindRepo } from "@/features/accounts/switcher/actions";
import { askAiLabel, helpRequest } from "@/features/ai/help/context";
import { askAi } from "@/features/ai/help/store";
import { aiKeys } from "@/features/ai/queries";
import { openAiSettings } from "@/features/ai/store";
import {
  accountLabel,
  hostMismatchFix,
  refusedHostOf,
  signInName,
} from "@/features/accounts/switcher/model";
import {
  followAccountRemote,
  remoteAccountName,
  remoteWithHost,
} from "@/features/accounts/switcher/tracking";
import { openConflictsAction } from "@/features/branch-switch/actions";
import { fetchAll, focusCommitSummary, pushTo } from "@/features/command-palette/actions";
import { runOutcome } from "@/features/commits/outcome";
import { useRefDialog } from "@/features/commits/store";
import { openForkDialog } from "@/features/forks/store";
import { repoKeys } from "@/features/repos/queries";
import { useStashUi } from "@/features/stash/store";
import { invalidateRepo } from "@/features/worktree/queries";
import {
  commands,
  errorMessage,
  type Account,
  type AiSettingsView,
  type Provider,
  type RepoAccountContext,
  type RepoConfigFix,
  type RepoRefs,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi, type Notice, type NoticeAction } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { detectProblem, type Problem } from "./detect";
import { largeFileActions } from "./largeFiles";
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
      // A push dropped half-way is most often a push carrying big files: getting them out of
      // the way works better than a bigger buffer (offered next to it).
      return [
        ...(repoId !== undefined && /send-pack|push/i.test(rawText(notice))
          ? largeFileActions(repoId, notice).slice(0, 1)
          : []),
        ...configFix(notice, repoId, "postBuffer"),
      ];
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
      // Never "Retry": the same file would be refused again after the same long upload.
      return largeFileActions(repoId, notice);
    case "connectionFailed":
      return retryAction(notice);
    case "staleLease":
      // Someone pushed since the last fetch: fetching shows their commits; forcing again
      // would erase them, so the choice stays with the user.
      return [{ label: i18n.t("recovery.fetchFirst"), run: () => void fetchAll(repoId) }];
    case "noUpstream":
      return publishBranchActions(repoId);
    case "detachedHead":
      return [{ label: i18n.t("recovery.createBranch"), run: () => createBranchAtHead(repoId) }];
    case "protectedBranch":
      return [{ label: i18n.t("recovery.protectedBranch"), run: () => createBranchAtHead(repoId) }];
    case "operationInProgress":
    case "unmergedFiles":
      return [
        openConflictsAction(repoId),
        { label: i18n.t("recovery.abortOperation"), run: () => void abortOperation(repoId) },
      ];
    case "secretDetected":
      return [{ label: i18n.t("recovery.undoLastCommit"), run: () => void undoLastCommit(repoId) }];
    case "permissionDenied":
      return [{ label: i18n.t("recovery.fork"), run: () => openForkDialog(repoId) }];
    default:
      return [];
  }
}

const cachedRefs = (repoId: number) => queryClient.getQueryData<RepoRefs>(repoKeys.refs(repoId));

/** No upstream yet: push the current branch to `origin` (or the only remote) and track it. */
function publishBranchActions(repoId: number): NoticeAction[] {
  const refs = cachedRefs(repoId);
  const branch = refs?.head.branch;
  const remotes = refs?.remotes.map((r) => r.name) ?? [];
  const remote = remotes.includes("origin") ? "origin" : remotes[0];
  if (!branch || !remote) return [];
  return [
    {
      label: i18n.t("recovery.publishBranch", { branch, remote }),
      run: () => void pushTo(repoId, branch, remote, false),
    },
  ];
}

/** "Create a branch here": keeps the commits made on a detached HEAD or a protected branch. */
function createBranchAtHead(repoId: number) {
  const start = cachedRefs(repoId)?.head.oid ?? "HEAD";
  const label = start === "HEAD" ? start : start.slice(0, 7);
  useRefDialog.getState().open({ kind: "createBranch", repoId, start, label });
}

async function abortOperation(repoId: number) {
  await runOutcome(
    repoId,
    () => commands.operationAbort(repoId),
    (op) => i18n.t("ops.aborted", { op: i18n.t(`ops.kind.${op}`) }),
  );
}

/**
 * The server found a password or key in the last commit: take the commit back (its changes
 * stay in the working tree) so the secret can be removed before committing again.
 */
async function undoLastCommit(repoId: number) {
  const head = cachedRefs(repoId)?.head.oid;
  if (!head) return;
  try {
    const parent = (await commands.commitDetails(repoId, head)).parents[0];
    if (!parent) {
      useUi.getState().showNotice({ kind: "error", message: i18n.t("recovery.firstCommit") });
      return;
    }
    await runOutcome(
      repoId,
      () => commands.resetTo(repoId, parent, "mixed"),
      () => ({
        kind: "success",
        message: i18n.t("recovery.commitUndone"),
        hint: i18n.t("recovery.commitUndoneHint"),
      }),
    );
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
  }
}

/**
 * A github.com remote bound to a GitLab account (or the reverse): the app never sends a token
 * to another server, so git is refused. Says which remote and account in plain words and offers
 * every way out: push to the remote of that account instead, use an account of this server for
 * the remote (then retry), or sign in to that server.
 */
function hostMismatchNotice(notice: Notice, repoId: number | undefined): Notice | null {
  const host = refusedHostOf(rawText(notice));
  if (repoId === undefined || !host) return null;
  const fix = hostMismatchFix(
    queryClient.getQueryData<RepoAccountContext>(accountKeys.repoContext(repoId)),
    queryClient.getQueryData<Account[]>(accountKeys.list) ?? [],
    queryClient.getQueryData<Provider[]>(accountKeys.providers) ?? [],
    host,
  );
  if (!fix) return null;
  const { remote, target } = fix;
  const vars = {
    remote: remoteWithHost(remote),
    account: remoteAccountName(remote),
    accountHost: fix.accountHost ?? "?",
  };
  const actions: NoticeAction[] = [];
  const branch = queryClient.getQueryData<RepoRefs>(repoKeys.refs(repoId))?.head.branch;
  if (target && branch) {
    const m = { branch, current: remote, target };
    actions.push({
      label: i18n.t("recovery.hostMismatch.pushTarget", {
        target: remoteWithHost(target),
        account: remoteAccountName(target),
      }),
      run: () =>
        void followAccountRemote(repoId, m, false).then(
          (ok) => ok && void pushTo(repoId, branch, target.remote, false),
        ),
    });
  }
  for (const account of fix.candidates.slice(0, 2)) {
    actions.push({
      label: i18n.t("recovery.hostMismatch.useAccount", {
        account: accountLabel(account),
        remote: remote.remote,
      }),
      run: () =>
        void bindRepo(repoId, { accountId: account.id, remote: remote.remote }).then(
          (ok) => ok && notice.retry?.(),
        ),
    });
  }
  if (fix.signIn) {
    const signIn = fix.signIn;
    const providers = queryClient.getQueryData<Provider[]>(accountKeys.providers) ?? [];
    actions.push({
      label: i18n.t("failureHelp.action.signIn", { name: signInName(signIn, providers) }),
      run: () => useAccountDialogs.getState().openManager({ signIn }),
    });
  }
  return {
    ...notice,
    repoId,
    message: i18n.t("recovery.hostMismatch.title", vars),
    hint: i18n.t(target ? "recovery.hostMismatch.hintTarget" : "recovery.hostMismatch.hint", {
      ...vars,
      target: target ? remoteWithHost(target) : "",
    }),
    detail: rawText(notice),
    actions,
    durationMs: RECOVERY_NOTICE_MS,
  };
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
  return notice.kind === "error" ? withAskAi(enhance(notice)) : notice;
}

/**
 * Last way out of any error: "Hỏi AI" when an AI service is set up (it explains this error
 * with the facts of the repository), else, when the app has no fix to offer, setting one up.
 */
function withAskAi(notice: Notice): Notice {
  const actions = notice.actions ?? [];
  const config = queryClient.getQueryData<AiSettingsView>(aiKeys.settings)?.config;
  const label = config?.enabled ? askAiLabel() : actions.length ? null : i18n.t("aiHelp.setup");
  if (!label || actions.some((a) => a.label === label)) return notice;
  const next: Notice = {
    ...notice,
    repoId: notice.repoId ?? activeRepoId(),
    durationMs: notice.durationMs ?? RECOVERY_NOTICE_MS,
  };
  next.actions = [
    ...actions,
    {
      label,
      run: () => {
        useUi.getState().showNotice(null);
        if (config?.enabled) askAi(helpRequest(next));
        else openAiSettings();
      },
    },
  ];
  return next;
}

function enhance(notice: Notice): Notice {
  const repoId = notice.repoId ?? activeRepoId();
  const mismatch = hostMismatchNotice(notice, repoId);
  if (mismatch) return mismatch;
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
