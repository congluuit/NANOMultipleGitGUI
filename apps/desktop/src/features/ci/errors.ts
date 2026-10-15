/**
 * Plain-language text + a fix button for provider API failures (CI and forks, Phase 4.3):
 * missing scope → sign in again, SAML → authorise the token, 404 on a private repository →
 * use another account, rate limit → when to retry.
 */
import type { TFunction } from "i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openExternal } from "@/features/accounts/manager/openExternal";
import type { CiProblem, ProviderError } from "@/ipc";

export interface ErrorAction {
  label: string;
  run: () => void;
}

export interface DescribedProblem {
  text: string;
  action: ErrorAction | null;
}

const reauth = (t: TFunction, accountId: string): ErrorAction => ({
  label: t("ci.action.reauth"),
  run: () => useAccountDialogs.getState().openManager({ accountId, reauth: true }),
});

/** `HH:MM` of a Unix time. */
function clock(seconds: number, locale: string): string {
  return new Date(seconds * 1000).toLocaleTimeString(locale, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Text for a provider error seen with `accountId` (`account` = its display label). */
export function describeProviderError(
  t: TFunction,
  error: ProviderError,
  accountId: string,
  account: string,
  locale = "vi",
): DescribedProblem {
  switch (error.kind) {
    case "unauthorized":
      return {
        text: t("ci.providerError.unauthorized", { account }),
        action: reauth(t, accountId),
      };
    case "forbidden":
    case "notFound": {
      const hint = error.hint;
      if (hint?.kind === "missingScopes") {
        return {
          text: t("ci.providerError.missingScopes", { account, scopes: hint.required.join(", ") }),
          action: reauth(t, accountId),
        };
      }
      if (hint?.kind === "samlSsoRequired") {
        const url = hint.url;
        return {
          text: t("ci.providerError.saml", { account }),
          action: url
            ? { label: t("ci.action.authorizeSso"), run: () => void openExternal(url) }
            : null,
        };
      }
      if (hint?.kind === "oauthAppRestricted") {
        return {
          text: t("ci.providerError.oauthRestricted", {
            org: hint.organization ?? "?",
            account,
          }),
          action: reauth(t, accountId),
        };
      }
      if (hint?.kind === "tokenPermissionMissing") {
        return {
          text: t("ci.providerError.tokenPermission", { account }),
          action: reauth(t, accountId),
        };
      }
      return error.kind === "notFound"
        ? { text: t("ci.providerError.notFound", { account }), action: null }
        : {
            text: t("ci.providerError.forbidden", { account, message: error.message }),
            action: null,
          };
    }
    case "rateLimited": {
      const at =
        error.resetAt ??
        (error.retryAfterSecs != null
          ? Math.floor(Date.now() / 1000) + error.retryAfterSecs
          : null);
      return {
        text:
          at != null
            ? t("ci.providerError.rateLimitedAt", { time: clock(at, locale) })
            : t("ci.providerError.rateLimited"),
        action: null,
      };
    }
    case "timeout":
    case "network":
      return { text: t("ci.providerError.network"), action: null };
    case "server":
      return { text: t("ci.providerError.server", { status: error.status }), action: null };
    default:
      return {
        text: t("ci.providerError.other", { message: "message" in error ? error.message : "" }),
        action: null,
      };
  }
}

/**
 * Hint for a CI problem, or `null` when nothing should be shown (no hosted remote: the repo
 * simply has no CI to show).
 */
export function describeCiProblem(
  t: TFunction,
  problem: CiProblem | null,
  accountLabel: (accountId: string) => string,
  locale = "vi",
): DescribedProblem | null {
  if (!problem) return null;
  switch (problem.kind) {
    case "noHostedRemote":
      return null;
    case "unbound":
      return { text: t("ci.problem.unbound", { remote: problem.remote }), action: null };
    case "noToken":
      return {
        text: t("ci.problem.noToken", { account: accountLabel(problem.accountId) }),
        action: reauth(t, problem.accountId),
      };
    case "provider":
      return describeProviderError(
        t,
        problem.error,
        problem.accountId,
        accountLabel(problem.accountId),
        locale,
      );
  }
}
