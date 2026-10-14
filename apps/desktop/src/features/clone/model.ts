/**
 * Pure logic of the clone dialog (PLAN §4.7): filtering/sorting the unified repository list,
 * the destination template preview, relative times, the disk-space check and form validation.
 */
import { accountLabel, providerNameOf } from "@/features/accounts/switcher/model";
import type {
  Account,
  CloneDestination,
  CloneGuess,
  CloneRepo,
  CloneRepoList,
  LfsMode,
  Provider,
} from "@/ipc";

/** Rows rendered at most (refine the search for more). */
export const MAX_ROWS = 200;

/** "Công ty (GitLab acme)". */
export function accountTitle(account: Account | undefined, providers: Provider[] | undefined) {
  if (!account) return "?";
  const provider = providerNameOf(account, providers);
  return provider ? `${accountLabel(account)} (${provider})` : accountLabel(account);
}

/** Every repository of every account, in account order. */
export function flattenRepos(list: CloneRepoList | undefined): CloneRepo[] {
  return list?.accounts.flatMap((a) => a.repos) ?? [];
}

export interface RepoFilter {
  query: string;
  /** `null` = every account. */
  accountId: string | null;
}

const words = (query: string) =>
  query
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter((w) => w.length > 0);

/** Higher is better; `-1` = no match. Name matches beat owner/description matches. */
function score(repo: CloneRepo, terms: string[]): number {
  if (terms.length === 0) return 0;
  const name = repo.name.toLocaleLowerCase();
  const full = repo.fullName.toLocaleLowerCase();
  const desc = (repo.description ?? "").toLocaleLowerCase();
  let total = 0;
  for (const term of terms) {
    if (name === term) total += 4;
    else if (name.startsWith(term)) total += 3;
    else if (name.includes(term)) total += 2;
    else if (full.includes(term)) total += 1;
    else if (desc.includes(term)) total += 0.5;
    else return -1;
  }
  return total;
}

/**
 * Repositories matching every word of `query` (name, `owner/name`, description) of the chosen
 * account. Without a query: most recently updated first; with one: best match first.
 */
export function filterRepos(repos: readonly CloneRepo[], filter: RepoFilter): CloneRepo[] {
  const terms = words(filter.query);
  return repos
    .filter((r) => filter.accountId === null || r.accountId === filter.accountId)
    .map((repo) => ({ repo, score: score(repo, terms) }))
    .filter((x) => x.score >= 0)
    .sort(
      (a, b) =>
        b.score - a.score ||
        (b.repo.updatedAt ?? 0) - (a.repo.updatedAt ?? 0) ||
        a.repo.fullName.localeCompare(b.repo.fullName),
    )
    .map((x) => x.repo);
}

/** Values substituted into the destination template. */
export interface TemplateVars {
  cloneDir: string;
  owner: string;
  repo: string;
  host: string;
  account: string;
}

/** Same rules as the backend: reserved/control characters → `_`, no `.`/`..` segments. */
export function sanitizeSegments(value: string): string {
  return value
    .split(/[/\\]/)
    .filter((s) => s.length > 0)
    .map((seg) => {
      // eslint-disable-next-line no-control-regex
      const cleaned = seg.replace(/[\u0000-\u001f\u007f<>:"|?*]/g, "_");
      const trimmed = cleaned.replace(/[. ]+$/, "").trimStart();
      return trimmed.length === 0 ? "_" : trimmed;
    })
    .join("/");
}

const isAbsolute = (p: string) => /^([a-zA-Z]:[/\\]|[/\\])/.test(p);

/**
 * Destination preview for `template` (`{clone_dir}`, `{owner}`, `{repo}`, `{host}`,
 * `{account}`), mirroring `render_template` in the backend. Uses the separator of `cloneDir`.
 */
export function renderTemplate(template: string, vars: TemplateVars): string {
  const sep = vars.cloneDir.includes("\\") ? "\\" : "/";
  const cloneDir = vars.cloneDir.replace(/[/\\]+$/, "");
  let s = template
    .replaceAll("{owner}", sanitizeSegments(vars.owner))
    .replaceAll("{repo}", sanitizeSegments(vars.repo))
    .replaceAll("{host}", sanitizeSegments(vars.host.replaceAll(":", "_")))
    .replaceAll("{account}", sanitizeSegments(vars.account));
  if (s.includes("{clone_dir}")) s = s.replaceAll("{clone_dir}", cloneDir);
  else if (!isAbsolute(s)) s = `${cloneDir}/${s}`;
  const unc = /^[/\\]{2}/.test(s);
  let out = s.replace(/[/\\]+/g, sep);
  if (unc) out = sep + out;
  while (out.length > 3 && out.endsWith(sep)) out = out.slice(0, -1);
  return out;
}

export const templateVars = (d: CloneDestination): TemplateVars => ({
  cloneDir: d.cloneDir,
  owner: d.owner,
  repo: d.repo,
  host: d.host,
  account: d.account,
});

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/** "2 giờ trước" / "2 hours ago" for a Unix timestamp in seconds. */
export function relativeTime(unixSecs: number, nowMs: number, language: string): string {
  const diff = Math.round(unixSecs - nowMs / 1000);
  const fmt = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  for (const [unit, secs] of UNITS) {
    if (Math.abs(diff) >= secs) return fmt.format(Math.round(diff / secs), unit);
  }
  return fmt.format(0, "minute");
}

/**
 * Bytes the clone needs on disk: the repository plus, when LFS files are downloaded, the LFS
 * objects twice (local store + working tree). `null` when the provider did not say.
 */
export function neededBytes(
  dest: Pick<CloneDestination, "sizeBytes" | "lfsSizeBytes">,
  lfs: LfsMode["mode"],
): number | null {
  if (dest.sizeBytes == null && dest.lfsSizeBytes == null) return null;
  const lfsBytes = lfs === "all" ? (dest.lfsSizeBytes ?? 0) * 2 : 0;
  return (dest.sizeBytes ?? 0) + lfsBytes;
}

/** Below this much free space we always warn. */
export const LOW_SPACE_BYTES = 1024 ** 3;

export function spaceWarning(
  freeBytes: number | null,
  needed: number | null,
): "insufficient" | "low" | null {
  if (freeBytes == null) return null;
  if (needed != null && freeBytes < needed) return "insufficient";
  if (freeBytes < LOW_SPACE_BYTES) return "low";
  return null;
}

export type Protocol = "https" | "ssh";

/** URL to clone for the chosen protocol (falls back to what was typed). */
export function urlForProtocol(
  typed: string,
  guess: Pick<CloneGuess, "httpsUrl" | "sshUrl" | "local"> | undefined,
  protocol: Protocol,
): string {
  if (!guess || guess.local) return typed.trim();
  return (protocol === "ssh" ? guess.sshUrl : guess.httpsUrl) ?? typed.trim();
}

export interface CloneForm {
  url: string;
  dest: string;
  lfs: "all" | "later" | "partial";
  lfsInclude: string;
}

export interface FormState {
  urlValid: boolean;
  destUsable: boolean;
}

export type FormError =
  "urlRequired" | "urlInvalid" | "destRequired" | "destNotEmpty" | "patternInvalid";

/** i18n keys (under `clone.error`) of the problems that block the Clone button. */
export function validateForm(form: CloneForm, state: FormState): FormError[] {
  const errors: FormError[] = [];
  if (form.url.trim().length === 0) errors.push("urlRequired");
  else if (!state.urlValid) errors.push("urlInvalid");
  if (form.dest.trim().length === 0) errors.push("destRequired");
  else if (!state.destUsable) errors.push("destNotEmpty");
  if (form.lfs === "partial") {
    const pattern = form.lfsInclude.trim();
    if (
      pattern.length === 0 ||
      pattern.startsWith("-") ||
      pattern.split(",").some((p) => !p.trim())
    )
      errors.push("patternInvalid");
  }
  return errors;
}

export function lfsMode(form: Pick<CloneForm, "lfs" | "lfsInclude">): LfsMode {
  if (form.lfs === "partial") return { mode: "partial", include: form.lfsInclude.trim() };
  return { mode: form.lfs };
}

/** Last path segment of a folder path. */
export const baseName = (path: string) => path.split(/[/\\]/).filter(Boolean).pop() ?? path;
