import i18n from "@/i18n";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { cachedAccounts, accountLabel } from "@/features/accounts/switcher";
import { runOutcome } from "@/features/commits/outcome";
import { commands, type ProviderFamily } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useHostingNav, type HostedItemRef } from "@/stores/hosting";
import type { Notice, NoticeAction } from "@/stores/ui";
import { describeIssueError, type HostingProblem } from "./errors";
import type { TextPart } from "./model";
import { issueKeys } from "./queries";

/** Label of an account id from the query cache (the id while accounts load). */
export function accountName(accountId: string): string {
  const account = cachedAccounts().accounts.find((a) => a.id === accountId);
  return account ? accountLabel(account) : accountId;
}

/** Buttons for the fix a problem suggests. */
export function problemActions(problem: HostingProblem, accountId: string): NoticeAction[] {
  const t = i18n.t.bind(i18n);
  if (problem.fix === "reauth") {
    return [
      {
        label: t("issues.signInAgain"),
        run: () => useAccountDialogs.getState().openManager({ accountId, reauth: true }),
      },
    ];
  }
  if (problem.fix === "openUrl" && problem.url) {
    const url = problem.url;
    return [{ label: t("issues.authorizeSso"), run: () => void openExternal(url) }];
  }
  if (problem.fix === "switchAccount") {
    return [
      {
        label: t("issues.manageAccounts"),
        run: () => useAccountDialogs.getState().openManager({ accountId }),
      },
    ];
  }
  return [];
}

/** Error notice (with the fix) for a failed issue command run as `accountId`. */
export function issueErrorNotice(e: unknown, accountId: string): Notice | null {
  const t = i18n.t.bind(i18n);
  const problem = describeIssueError(t, e, accountName(accountId), i18n.language);
  if (problem.cancelled) return null;
  return {
    kind: "error",
    message: problem.message,
    hint: problem.hint,
    actions: problemActions(problem, accountId),
  };
}

/** Where a `#12` / `!12` of a text belongs. */
export interface RefContext {
  accountId: string;
  fullName: string;
  family: ProviderFamily | null;
  repoId: number | null;
}

/**
 * Opens `#12` / `!12`: GitLab `!` is a merge request and `#` an issue; on GitHub numbers are
 * shared, so the issue endpoint tells whether it is a pull request.
 */
export async function openRef(ctx: RefContext, part: Extract<TextPart, { type: "ref" }>) {
  const ref: HostedItemRef = {
    accountId: ctx.accountId,
    fullName: ctx.fullName,
    number: part.number,
    repoId: ctx.repoId,
  };
  const nav = useHostingNav.getState();
  if (part.mergeRequest) return nav.openPullRequest(ref);
  if (ctx.family === "github") {
    try {
      const detail = await queryClient.fetchQuery({
        queryKey: issueKeys.detail(ref),
        queryFn: () => commands.issueGet(ref.accountId, ref.fullName, ref.number),
        staleTime: 30_000,
      });
      if (detail.issue.isPullRequest) return nav.openPullRequest(ref);
    } catch {
      // The issue view shows the error (and how to fix it).
    }
  }
  nav.openIssue(ref);
}

/** Creates the local branch for an issue; returns its name when it was created. */
export async function createIssueBranch(
  repoId: number,
  number: number,
  title: string,
  name: string,
  checkout: boolean,
): Promise<string | undefined> {
  const t = i18n.t.bind(i18n);
  return runOutcome(
    repoId,
    () => commands.issueBranchCreate(repoId, number, title, name.trim() || null, null, checkout),
    (branch) =>
      checkout
        ? t("issues.branch.createdCheckedOut", { branch })
        : t("issues.branch.created", { branch }),
  );
}
