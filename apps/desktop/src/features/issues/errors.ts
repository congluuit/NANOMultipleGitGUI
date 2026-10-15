import type { TFunction } from "i18next";
import { errorMessage, type IssueCommandError, type ProviderError } from "@/ipc";

/** What the user can do about a failed provider call. */
export type HostingFix =
  /** Sign in again / replace the token (expired, revoked, missing scope). */
  | "reauth"
  /** Try another account (404 on a private project, 403). */
  | "switchAccount"
  /** Wait until `retryAt` (rate limit). */
  | "wait"
  /** Open a URL (SAML SSO authorization page). */
  | "openUrl";

export interface HostingProblem {
  message: string;
  /** Plain-words explanation of how to fix it. */
  hint?: string;
  fix?: HostingFix;
  url?: string;
  /** The call was cancelled on purpose: show nothing. */
  cancelled?: boolean;
}

const KINDS = ["provider", "app"] as const;

export function isIssueCommandError(e: unknown): e is IssueCommandError {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "error" in e &&
    (KINDS as readonly string[]).includes((e as { kind: string }).kind)
  );
}

/** `HH:mm` of a Unix time (seconds), for "try again at …". */
function clock(unixSecs: number, language: string): string {
  return new Date(unixSecs * 1000).toLocaleTimeString(language, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function retryAt(e: Extract<ProviderError, { kind: "rateLimited" }>, nowMs: number) {
  if (e.resetAt) return e.resetAt;
  if (e.retryAfterSecs) return Math.round(nowMs / 1000) + e.retryAfterSecs;
  return null;
}

/**
 * Translated explanation of a provider error for account `account` (its label), with the fix
 * the UI should offer.
 */
export function describeProviderError(
  t: TFunction,
  e: ProviderError,
  account: string,
  language = "en",
  nowMs = Date.now(),
): HostingProblem {
  const hint = "hint" in e ? e.hint : null;
  switch (hint?.kind) {
    case "missingScopes":
      return {
        message: t("issues.error.missingScopes", { account, scopes: hint.required.join(", ") }),
        hint: t("issues.error.missingScopesHint"),
        fix: "reauth",
      };
    case "oauthAppRestricted":
      return {
        message: t("issues.error.oauthRestricted", {
          account,
          org: hint.organization ?? "",
        }),
        hint: t("issues.error.oauthRestrictedHint"),
        fix: "reauth",
      };
    case "samlSsoRequired":
      return {
        message: t("issues.error.saml", { account }),
        hint: t("issues.error.samlHint"),
        fix: hint.url?.startsWith("https://") ? "openUrl" : undefined,
        url: hint.url?.startsWith("https://") ? hint.url : undefined,
      };
    case "tokenPermissionMissing":
      return {
        message: t("issues.error.tokenPermission", { account }),
        hint: t("issues.error.tokenPermissionHint"),
        fix: "reauth",
      };
    default:
      break;
  }
  switch (e.kind) {
    case "unauthorized":
      return {
        message: t("issues.error.unauthorized", { account }),
        hint: t("issues.error.unauthorizedHint"),
        fix: "reauth",
      };
    case "notFound":
      return {
        message: t("issues.error.notFound", { account }),
        hint: t("issues.error.notFoundHint"),
        fix: "switchAccount",
      };
    case "forbidden":
      return {
        message: t("issues.error.forbidden", { account }),
        hint: e.message,
        fix: "switchAccount",
      };
    case "rateLimited": {
      const at = retryAt(e, nowMs);
      return {
        message: at
          ? t("issues.error.rateLimitedAt", { time: clock(at, language) })
          : t("issues.error.rateLimited"),
        hint: t("issues.error.rateLimitedHint"),
        fix: "wait",
      };
    }
    case "rejected":
      return { message: t("issues.error.rejected", { message: e.message }) };
    case "timeout":
    case "network":
      return {
        message: t("issues.error.network"),
        hint: t("issues.error.networkHint"),
      };
    case "server":
      return { message: t("issues.error.server", { status: e.status }) };
    case "cancelled":
      return { message: t("issues.error.cancelled"), cancelled: true };
    default:
      return { message: "message" in e ? e.message : e.kind };
  }
}

/** Anything an issue / "My work" command threw. */
export function describeIssueError(
  t: TFunction,
  e: unknown,
  account: string,
  language = "en",
  nowMs = Date.now(),
): HostingProblem {
  if (isIssueCommandError(e)) {
    if (e.kind === "provider") return describeProviderError(t, e.error, account, language, nowMs);
    if (e.error.kind === "unsupported") {
      return { message: e.message, hint: t("issues.error.appHint") };
    }
    return { message: e.message };
  }
  return { message: errorMessage(e) };
}
