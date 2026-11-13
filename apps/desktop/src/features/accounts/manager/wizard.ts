import type { Provider, ProviderKind } from "@/ipc";

export type AuthMethodChoice = "device" | "pkce" | "pat";

export type WizardStep = "provider" | "method" | "device" | "pkce" | "pat" | "done";

/** The four provider choices of the first step, in display order. */
export const PROVIDER_CHOICES: readonly ProviderKind[] = [
  "github",
  "githubEnterprise",
  "gitlab",
  "gitlabSelfHosted",
  "gitea",
  "bitbucket",
  "azureDevops",
];

/** Servers the app only signs git transfers for (no pull requests, issues or CI). */
export const isGitOnly = (kind: ProviderKind) => kind === "bitbucket" || kind === "azureDevops";

const FIXED_BASE_URLS: Partial<Record<ProviderKind, string>> = {
  bitbucket: "https://bitbucket.org",
  azureDevops: "https://dev.azure.com",
};

/** Base URL of a cloud service that is not a built-in provider row (created on first use). */
export const fixedBaseUrl = (kind: ProviderKind): string | null => FIXED_BASE_URLS[kind] ?? null;

export const isSelfHosted = (kind: ProviderKind) =>
  kind === "githubEnterprise" || kind === "gitlabSelfHosted" || kind === "gitea";

export const isGitlab = (kind: ProviderKind) => kind === "gitlab" || kind === "gitlabSelfHosted";

/** Sign-in methods offered for a provider kind, recommended first. */
export function methodsFor(kind: ProviderKind): AuthMethodChoice[] {
  // Gitea / Forgejo: access token only (no OAuth app is registered there).
  if (kind === "gitea" || isGitOnly(kind)) return ["pat"];
  return isGitlab(kind) ? ["pkce", "device", "pat"] : ["device", "pat"];
}

export type UrlProblem = "required" | "invalid" | "insecure";

/** `git.acme.vn/gitlab/` → `https://git.acme.vn/gitlab`. */
export function normalizeBaseUrl(input: string): string {
  const trimmed = input.trim();
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  return withScheme.replace(/\/+$/, "");
}

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]"]);

/** Validation of a self-hosted base URL (https only, except loopback test servers). */
export function validateBaseUrl(input: string): UrlProblem | null {
  if (!input.trim()) return "required";
  let url: URL;
  try {
    url = new URL(normalizeBaseUrl(input));
  } catch {
    return "invalid";
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return "invalid";
  if (!url.hostname || url.username || url.password || url.search || url.hash) return "invalid";
  if (url.protocol === "http:" && !LOOPBACK.has(url.hostname)) return "insecure";
  return null;
}

export type TokenProblem = "required" | "malformed";

export function validateToken(token: string): TokenProblem | null {
  const t = token.trim();
  if (!t) return "required";
  if (/\s/.test(t)) return "malformed";
  return null;
}

const sameUrl = (a: string, b: string) =>
  a.replace(/\/+$/, "").toLowerCase() === b.replace(/\/+$/, "").toLowerCase();

/**
 * The provider row to sign in with: the built-in one for github.com / gitlab.com, an existing
 * instance with the same base URL, or `null` (create it with `provider_save`).
 */
export function findProvider(
  providers: readonly Provider[],
  kind: ProviderKind,
  baseUrl: string,
): Provider | null {
  const fixed = fixedBaseUrl(kind);
  if (!isSelfHosted(kind) && !fixed) {
    return providers.find((p) => p.kind === kind && p.builtin) ?? null;
  }
  const url = fixed ?? normalizeBaseUrl(baseUrl);
  return providers.find((p) => p.kind === kind && sameUrl(p.baseUrl, url)) ?? null;
}

/** Default name of a new instance: "GitLab git.acme.vn". */
export function instanceName(kind: ProviderKind, baseUrl: string): string {
  if (kind === "bitbucket") return "Bitbucket";
  if (kind === "azureDevops") return "Azure DevOps";
  let host = baseUrl;
  try {
    host = new URL(normalizeBaseUrl(baseUrl)).host;
  } catch {
    // keep the raw input
  }
  const name = kind === "gitea" ? "Gitea" : isGitlab(kind) ? "GitLab" : "GitHub Enterprise";
  return `${name} ${host}`;
}

/** Remaining time of a device code as `m:ss`; `null` once expired. */
export function countdown(expiresAtSecs: number, nowMs: number): string | null {
  const left = Math.floor(expiresAtSecs - nowMs / 1000);
  if (left <= 0) return null;
  const m = Math.floor(left / 60);
  const s = left % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
