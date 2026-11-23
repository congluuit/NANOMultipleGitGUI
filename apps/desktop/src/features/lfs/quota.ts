/**
 * Git LFS quotas of the hosting providers, for the Overview tab of the LFS manager.
 *
 * The numbers come from the providers' own documentation, read on 2026-11-23. Providers change
 * them from time to time, so each entry keeps the page it was read from (`docsUrl`) and the UI
 * links to it next to the numbers.
 *
 * - GitHub: https://docs.github.com/en/billing/concepts/product-billing/git-lfs (included
 *   storage and bandwidth per plan, what happens at $0 budget / no payment method) and
 *   https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage
 *   (largest LFS file per plan). Removing an object from history does not free storage: only
 *   deleting the repository or GitHub Support does
 *   (https://docs.github.com/en/repositories/working-with-files/managing-large-files/removing-files-from-git-large-file-storage).
 * - GitLab.com: https://docs.gitlab.com/user/storage_usage_quotas/ and
 *   https://docs.gitlab.com/user/gitlab_com/ (10 GiB per Free project, 500 GiB per
 *   Premium / Ultimate project, repository and LFS together; over the limit the project turns
 *   read-only). No LFS bandwidth quota and no per-file LFS limit is documented.
 * - Bitbucket Cloud: https://support.atlassian.com/bitbucket-cloud/docs/storage-policy-for-git-lfs-with-bitbucket/
 *   (1 GB Free, 5 GB Standard, 10 GB Premium per workspace; more in units of 100 GB; "no limit
 *   on the LFS file size").
 * - Azure DevOps: https://learn.microsoft.com/en-us/azure/devops/repos/git/manage-large-files
 *   (LFS offered for free, no quota stated) and .../repos/git/limits.
 * - Gitea / Forgejo: the server admin decides (`[lfs] LFS_MAX_FILE_SIZE`, default 0 = no limit;
 *   Forgejo quotas are off by default): https://docs.gitea.com/administration/config-cheat-sheet,
 *   https://forgejo.org/docs/latest/admin/quota/.
 * - Codeberg: https://docs.codeberg.org/getting-started/faq/ (ask first before using more than
 *   1.5 GiB of packages, LFS and attachments).
 */
import type { LfsFile, ProviderKind } from "@/ipc";

/** Which provider's rules apply to a remote. */
export type LfsHost =
  | "github"
  | "githubEnterprise"
  | "gitlab"
  | "gitlabSelfHosted"
  | "bitbucket"
  | "azure"
  | "codeberg"
  | "gitea"
  | "unknown";

/** A size as the provider writes it (GiB vs GB matters for the label only). */
export interface QuotaSize {
  bytes: number;
  label: string;
}

const GIB = 1024 ** 3;
const gib = (n: number): QuotaSize => ({ bytes: n * GIB, label: `${n} GiB` });
const gb = (n: number): QuotaSize => ({ bytes: n * 1e9, label: `${n} GB` });

export interface PlanQuota {
  plan: string;
  /** Included LFS storage; `null` = none stated. */
  storage: QuotaSize | null;
  /** Included download bandwidth per month; `null` = none stated. */
  bandwidth: QuotaSize | null;
  /** Largest single LFS file; `null` = no limit stated. */
  fileMax: QuotaSize | null;
}

export interface ProviderQuota {
  host: LfsHost;
  /** Product name shown as is. */
  name: string;
  /** Empty when the server's admin sets the limits. */
  plans: PlanQuota[];
  /** Official page the numbers come from. */
  docsUrl: string;
}

const QUOTAS: Record<LfsHost, ProviderQuota> = {
  github: {
    host: "github",
    name: "GitHub",
    plans: [
      { plan: "Free / Pro", storage: gib(10), bandwidth: gib(10), fileMax: gb(2) },
      { plan: "Team", storage: gib(250), bandwidth: gib(250), fileMax: gb(4) },
      { plan: "Enterprise Cloud", storage: gib(250), bandwidth: gib(250), fileMax: gb(5) },
    ],
    docsUrl: "https://docs.github.com/en/billing/concepts/product-billing/git-lfs",
  },
  githubEnterprise: {
    host: "githubEnterprise",
    name: "GitHub Enterprise Server",
    plans: [],
    docsUrl:
      "https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage",
  },
  gitlab: {
    host: "gitlab",
    name: "GitLab.com",
    plans: [
      { plan: "Free", storage: gib(10), bandwidth: null, fileMax: null },
      { plan: "Premium / Ultimate", storage: gib(500), bandwidth: null, fileMax: null },
    ],
    docsUrl: "https://docs.gitlab.com/user/storage_usage_quotas/",
  },
  gitlabSelfHosted: {
    host: "gitlabSelfHosted",
    name: "GitLab (self-hosted)",
    plans: [],
    docsUrl: "https://docs.gitlab.com/administration/settings/account_and_limit_settings/",
  },
  bitbucket: {
    host: "bitbucket",
    name: "Bitbucket Cloud",
    plans: [
      { plan: "Free", storage: gb(1), bandwidth: null, fileMax: null },
      { plan: "Standard", storage: gb(5), bandwidth: null, fileMax: null },
      { plan: "Premium", storage: gb(10), bandwidth: null, fileMax: null },
    ],
    docsUrl:
      "https://support.atlassian.com/bitbucket-cloud/docs/storage-policy-for-git-lfs-with-bitbucket/",
  },
  azure: {
    host: "azure",
    name: "Azure DevOps",
    plans: [],
    docsUrl: "https://learn.microsoft.com/en-us/azure/devops/repos/git/manage-large-files",
  },
  codeberg: {
    host: "codeberg",
    name: "Codeberg",
    plans: [{ plan: "Codeberg", storage: gib(1.5), bandwidth: null, fileMax: null }],
    docsUrl: "https://docs.codeberg.org/getting-started/faq/",
  },
  gitea: {
    host: "gitea",
    name: "Gitea / Forgejo",
    plans: [],
    docsUrl: "https://docs.gitea.com/administration/config-cheat-sheet",
  },
  unknown: {
    host: "unknown",
    name: "",
    plans: [],
    docsUrl: "https://git-lfs.com/",
  },
};

export function providerQuota(host: LfsHost): ProviderQuota {
  return QUOTAS[host];
}

const BY_KIND: Record<ProviderKind, LfsHost> = {
  github: "github",
  githubEnterprise: "githubEnterprise",
  gitlab: "gitlab",
  gitlabSelfHosted: "gitlabSelfHosted",
  gitea: "gitea",
  bitbucket: "bitbucket",
  azureDevops: "azure",
};

/**
 * Provider rules for a remote host: well-known hosts by name, then the kind of the account
 * bound to the remote (a self-hosted server), then a guess from the host name.
 */
export function lfsHostOf(
  host: string | null | undefined,
  kind: ProviderKind | null | undefined,
): LfsHost {
  const h = (host ?? "").toLowerCase();
  if (h === "github.com" || h.endsWith(".github.com")) return "github";
  if (h === "gitlab.com") return "gitlab";
  if (h === "bitbucket.org") return "bitbucket";
  if (h === "codeberg.org") return "codeberg";
  if (h === "dev.azure.com" || h.endsWith(".dev.azure.com") || h.endsWith(".visualstudio.com"))
    return "azure";
  if (kind) {
    const byKind = BY_KIND[kind];
    // An account of kind "gitlab" on another host is a self-hosted GitLab.
    return byKind === "gitlab" ? "gitlabSelfHosted" : byKind;
  }
  if (h.includes("gitlab")) return "gitlabSelfHosted";
  if (h.includes("gitea") || h.includes("forgejo")) return "gitea";
  return "unknown";
}

/** Page where the owner sees the LFS usage of `fullName` on the provider, if there is one. */
export function usagePageUrl(
  host: LfsHost,
  remoteHost: string | null | undefined,
  fullName: string | null | undefined,
  organization: boolean | null,
): string | null {
  const owner = fullName?.split("/")[0];
  switch (host) {
    case "github":
      return organization && owner
        ? `https://github.com/organizations/${owner}/settings/billing/usage`
        : "https://github.com/settings/billing/usage";
    case "gitlab":
    case "gitlabSelfHosted":
      return remoteHost && fullName ? `https://${remoteHost}/${fullName}/-/usage_quotas` : null;
    default:
      return null;
  }
}

/** The `n` biggest LFS files, biggest first. */
export function largestFiles(files: readonly LfsFile[], n = 5): LfsFile[] {
  return [...files].sort((a, b) => b.size - a.size || a.path.localeCompare(b.path)).slice(0, n);
}

/** The smallest per-file limit across the plans (the one a free account hits first). */
export function smallestFileMax(quota: ProviderQuota): QuotaSize | null {
  let out: QuotaSize | null = null;
  for (const p of quota.plans) {
    if (p.fileMax && (!out || p.fileMax.bytes < out.bytes)) out = p.fileMax;
  }
  return out;
}

/** Files bigger than `limit` (none when there is no limit). */
export function filesOver(files: readonly LfsFile[], limit: QuotaSize | null): LfsFile[] {
  return limit ? files.filter((f) => f.size > limit.bytes) : [];
}

/** `used / total` in percent, 0 to 100, for a progress bar. */
export function percentOf(used: number, total: number): number {
  if (!(total > 0) || !(used > 0)) return 0;
  return Math.min(100, Math.round((used / total) * 1000) / 10);
}
