import type { TFunction } from "i18next";
import { errorMessage, type AuthCommandError, type AuthError, type ProviderError } from "@/ipc";

export interface DescribedError {
  message: string;
  /** No OAuth client id: offer a PAT with this page / these scopes. */
  pat?: { url: string; scopes: string[] };
  /** The device flow is not available: offer PKCE or a PAT. */
  deviceUnsupported?: boolean;
  /** GitHub org blocks OAuth apps: suggest a fine-grained PAT. */
  oauthRestricted?: boolean;
  cancelled?: boolean;
}

const AUTH_KINDS = [
  "missingClientId",
  "unsupported",
  "auth",
  "unknownFlow",
  "wrongAccount",
  "app",
] as const;

export function isAuthCommandError(e: unknown): e is AuthCommandError {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    (AUTH_KINDS as readonly string[]).includes((e as { kind: string }).kind)
  );
}

function providerMessage(t: TFunction, e: ProviderError, fallback: string): string {
  switch (e.kind) {
    case "unauthorized":
      return t("auth.error.unauthorized");
    case "forbidden":
      return e.hint?.kind === "oauthAppRestricted"
        ? t("auth.error.oauthRestricted")
        : t("auth.error.forbidden", { message: e.message });
    case "rateLimited":
      return t("auth.error.rateLimited");
    case "timeout":
    case "network":
      return t("auth.error.network");
    default:
      return fallback;
  }
}

function authMessage(t: TFunction, e: AuthError, fallback: string): string {
  switch (e.kind) {
    case "denied":
      return t("auth.error.denied");
    case "expired":
      return t("auth.error.expired");
    case "timedOut":
      return t("auth.error.timedOut");
    case "cancelled":
      return t("auth.error.cancelled");
    case "deviceFlowUnsupported":
      return t("auth.error.deviceUnsupported");
    case "stateMismatch":
      return t("auth.error.stateMismatch");
    case "refreshRejected":
    case "tokenRevoked":
      return t("auth.error.revoked");
    case "loopback":
      return t("auth.error.loopback", { message: e.message });
    case "provider":
      return providerMessage(t, e.error, fallback);
    default:
      return fallback;
  }
}

/** Translated text (and suggested fallback) for anything an auth command threw. */
export function describeAuthError(t: TFunction, e: unknown): DescribedError {
  if (!isAuthCommandError(e)) return { message: errorMessage(e) };
  switch (e.kind) {
    case "missingClientId":
      return {
        message: t("auth.error.missingClientId"),
        pat: { url: e.patUrl, scopes: e.patScopes },
      };
    case "unsupported":
      return { message: t("auth.error.pkceUnsupported") };
    case "unknownFlow":
      return { message: t("auth.error.unknownFlow") };
    case "wrongAccount":
      return {
        message: t("auth.error.wrongAccount", { expected: e.expected, actual: e.actual }),
      };
    case "app":
      return { message: e.message };
    case "auth":
      return {
        message: authMessage(t, e.error, e.message),
        deviceUnsupported: e.error.kind === "deviceFlowUnsupported",
        oauthRestricted:
          e.error.kind === "provider" &&
          (e.error.error.kind === "forbidden" || e.error.error.kind === "notFound") &&
          e.error.error.hint?.kind === "oauthAppRestricted",
        cancelled: e.error.kind === "cancelled",
      };
  }
}
