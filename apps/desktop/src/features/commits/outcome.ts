import i18n from "i18next";
import { errorMessage, type Account, type FailureKind, type OpOutcome, type Outcome } from "@/ipc";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { cachedRepoAccount } from "@/features/accounts/switcher/hooks";
import { invalidateRepo } from "@/features/worktree/queries";
import { useUi, type Notice, type NoticeAction } from "@/stores/ui";

/** User-facing text for a rejected operation: our explanation, then git's own message. */
export function failureText(reason: FailureKind, message: string): string {
  const label = i18n.t(`failure.${reason}`);
  const detail = message.trim();
  return detail && detail !== label ? `${label} — ${detail}` : label;
}

/**
 * Notice for a rejected operation: what happened, how to fix it in plain words, buttons for
 * the fix when the app can do it, and git's raw output folded away.
 */
export function failureNotice(reason: FailureKind, message: string, repoId?: number): Notice {
  const account = repoId === undefined ? null : cachedRepoAccount(repoId).account;
  const name = account ? (account.label ?? account.username) : i18n.t("failureHelp.thisAccount");
  const label = i18n.t(`failure.${reason}`);
  const detail = message.trim();
  return {
    kind: "error",
    message: label,
    hint: i18n.t(`failureHelp.${reason}`, { account: name }),
    detail: detail && detail !== label ? detail : undefined,
    actions: failureActions(reason, account),
  };
}

function failureActions(reason: FailureKind, account: Account | null): NoticeAction[] {
  const dialogs = useAccountDialogs.getState();
  switch (reason) {
    case "workflowScope":
    case "authFailed":
      return [
        account
          ? {
              label: i18n.t("failureHelp.action.reauth", {
                account: account.label ?? account.username,
              }),
              run: () => dialogs.openManager({ accountId: account.id, reauth: true }),
            }
          : {
              label: i18n.t("failureHelp.action.accounts"),
              run: () => dialogs.openManager(),
            },
      ];
    case "permissionDenied":
    case "signingFailed":
      return [
        {
          label: i18n.t("failureHelp.action.accounts"),
          run: () => dialogs.openManager({ accountId: account?.id ?? null }),
        },
      ];
    case "emailPrivacy":
      return [
        {
          label: i18n.t("failureHelp.action.emailSettings"),
          run: () => void openExternal("https://github.com/settings/emails"),
        },
      ];
    default:
      return [];
  }
}

/** Notice for an operation that stopped on conflicts (empty string when there are none). */
export function conflictText(outcome: Pick<OpOutcome, "conflicts">): string {
  return outcome.conflicts.length
    ? i18n.t("ops.conflicts", { count: outcome.conflicts.length })
    : "";
}

/**
 * Runs a write command returning an `Outcome`, reports rejections/cancellation/errors in the
 * notice bar, refreshes the repository and returns the value when it went through.
 */
export async function runOutcome<T>(
  repoId: number,
  call: () => Promise<Outcome<T>>,
  success?: (value: T) => string | Notice | null,
): Promise<T | undefined> {
  const notify = useUi.getState().showNotice;
  try {
    const out = await call();
    if (out.status === "done") {
      const done = success?.(out.value);
      if (done) notify(typeof done === "string" ? { kind: "info", message: done } : done);
      return out.value;
    }
    if (out.status === "rejected") {
      notify(failureNotice(out.reason, out.message, repoId));
    } else {
      notify({ kind: "info", message: i18n.t("ops.cancelled") });
    }
  } catch (e) {
    notify({ kind: "error", message: errorMessage(e) });
  } finally {
    invalidateRepo(repoId);
  }
  return undefined;
}
