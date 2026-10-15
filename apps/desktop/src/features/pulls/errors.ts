import i18n from "i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { accountLabel, cachedAccounts } from "@/features/accounts/switcher";
import { errorMessage, type PullCommandError } from "@/ipc";
import { useUi, type Notice, type NoticeAction } from "@/stores/ui";

export function isPullCommandError(e: unknown): e is PullCommandError {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    ((e as { kind: string }).kind === "provider" || (e as { kind: string }).kind === "app") &&
    "error" in e
  );
}

export interface DescribedPullError {
  message: string;
  /** What to do, in plain words. */
  hint?: string;
  actions: NoticeAction[];
  /** The provider rejected the token: offer "sign in again". */
  reauth?: boolean;
}

function accountName(accountId: string): string {
  const a = cachedAccounts().accounts.find((x) => x.id === accountId);
  return a ? accountLabel(a) : i18n.t("pulls.error.thisAccount");
}

function clock(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleTimeString(i18n.language, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * Translated explanation of anything a `pull_*` command threw, with the fix (sign in again,
 * missing scope, SAML, rate limit…) and buttons when the app can help.
 */
export function describePullError(e: unknown): DescribedPullError {
  const t = i18n.t.bind(i18n);
  if (!isPullCommandError(e)) return { message: errorMessage(e), actions: [] };
  if (e.kind === "app") return { message: e.message, actions: [] };
  const account = accountName(e.accountId);
  const dialogs = useAccountDialogs.getState();
  const reauth: NoticeAction = {
    label: t("pulls.error.reauth", { account }),
    run: () => dialogs.openManager({ accountId: e.accountId, reauth: true }),
  };
  const manage: NoticeAction = {
    label: t("pulls.error.manage"),
    run: () => dialogs.openManager({ accountId: e.accountId }),
  };
  const err = e.error;
  const hint = err.kind === "forbidden" || err.kind === "notFound" ? err.hint : null;
  if (hint?.kind === "missingScopes") {
    return {
      message: t("pulls.error.missingScopes", { account, scopes: hint.required.join(", ") }),
      hint: t("pulls.error.missingScopesHint"),
      actions: [reauth],
      reauth: true,
    };
  }
  if (hint?.kind === "samlSsoRequired") {
    const url = hint.url;
    return {
      message: t("pulls.error.saml", { account }),
      hint: t("pulls.error.samlHint"),
      actions: url ? [{ label: t("pulls.error.samlOpen"), run: () => void openExternal(url) }] : [],
    };
  }
  if (hint?.kind === "oauthAppRestricted") {
    return {
      message: t("pulls.error.oauthRestricted", {
        account,
        org: hint.organization ?? "",
      }),
      hint: t("pulls.error.oauthRestrictedHint"),
      actions: [manage],
    };
  }
  if (hint?.kind === "tokenPermissionMissing") {
    return {
      message: t("pulls.error.tokenPermission", { account }),
      hint: t("pulls.error.tokenPermissionHint"),
      actions: [reauth],
      reauth: true,
    };
  }
  switch (err.kind) {
    case "unauthorized":
      return {
        message: t("pulls.error.unauthorized", { account }),
        hint: t("pulls.error.unauthorizedHint"),
        actions: [reauth],
        reauth: true,
      };
    case "forbidden":
      return {
        message: t("pulls.error.forbidden", { account }),
        hint: err.message,
        actions: [manage],
      };
    case "notFound":
      return {
        message: t("pulls.error.notFound", { account }),
        hint: t("pulls.error.notFoundHint"),
        actions: [manage],
      };
    case "rateLimited": {
      const at =
        err.resetAt ??
        (err.retryAfterSecs !== null ? Math.round(Date.now() / 1000) + err.retryAfterSecs : null);
      return {
        message: at
          ? t("pulls.error.rateLimitedAt", { time: clock(at) })
          : t("pulls.error.rateLimited"),
        actions: [],
      };
    }
    case "rejected":
    case "alreadyExists":
      return { message: t("pulls.error.rejected", { message: err.message }), actions: [] };
    case "unexpectedStatus":
      return { message: t("pulls.error.refused", { message: err.message }), actions: [] };
    case "timeout":
    case "network":
      return { message: t("pulls.error.network"), hint: e.message, actions: [] };
    case "server":
      return { message: t("pulls.error.server"), hint: err.message, actions: [] };
    case "unsupported":
      return { message: t("pulls.error.unsupported", { message: err.message }), actions: [] };
    default:
      return { message: e.message, actions: [] };
  }
}

export function pullErrorNotice(e: unknown): Notice {
  const d = describePullError(e);
  return { kind: "error", message: d.message, hint: d.hint, actions: d.actions };
}

/** Reports a failed write in the notice bar. */
export function notifyPullError(e: unknown) {
  useUi.getState().showNotice(pullErrorNotice(e));
}
