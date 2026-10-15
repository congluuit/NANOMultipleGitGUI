/**
 * Pure logic of forks (PLAN §4.7, Phase 4.3): what is being forked, whether the account owns it,
 * which accounts can fork from a host, remote names and failure texts.
 */
import type { TFunction } from "i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { describeProviderError, type DescribedProblem } from "@/features/ci/errors";
import type { Account, CloneGuess, CloneRepo, ForkOutcome, Provider } from "@/ipc";

/** The project to fork and its clone URLs (used for `upstream`). */
export interface ForkSource {
  /** `owner/repo` or `group/sub/project`. */
  fullName: string;
  /** Owner / group path. */
  owner: string;
  host: string | null;
  cloneHttps: string;
  cloneSsh: string;
}

/** Fork source of the clone dialog's selection (a listed repo or a pasted URL). */
export function forkSourceOf(
  selected: CloneRepo | null,
  guess: CloneGuess | undefined,
): ForkSource | null {
  if (selected) {
    return {
      fullName: selected.fullName,
      owner: selected.owner,
      host: hostOf(selected.cloneHttps),
      cloneHttps: selected.cloneHttps,
      cloneSsh: selected.cloneSsh,
    };
  }
  if (!guess?.valid || guess.local || !guess.owner || !guess.repo) return null;
  return {
    fullName: `${guess.owner}/${guess.repo}`,
    owner: guess.owner,
    host: guess.host,
    cloneHttps: guess.httpsUrl ?? "",
    cloneSsh: guess.sshUrl ?? "",
  };
}

/** Lower-cased host name of an http(s) URL. */
export function hostOf(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).hostname.toLowerCase() || null;
  } catch {
    return null;
  }
}

/** The account owns the project directly (its own namespace): forking makes no sense. */
export function ownedBy(source: Pick<ForkSource, "owner">, account: Account | undefined): boolean {
  return !!account && source.owner.toLowerCase() === account.username.toLowerCase();
}

/** Whether the clone dialog should offer forking `source` with `account`. */
export const canOfferFork = (source: ForkSource | null, account: Account | undefined) =>
  !!source && !!account && !ownedBy(source, account);

/** Accounts whose provider lives on `host` (forking with another host's account is impossible). */
export function accountsOnHost(
  accounts: readonly Account[],
  providers: readonly Provider[] | undefined,
  host: string | null,
): Account[] {
  if (!host) return [];
  const hosts = new Map((providers ?? []).map((p) => [p.id, hostOf(p.baseUrl)]));
  return accounts.filter((a) => hosts.get(a.providerId) === host.toLowerCase());
}

/** `fork`, or `fork-2`, `fork-3`… when taken. */
export function freeRemoteName(existing: readonly string[], base = "fork"): string {
  if (!existing.includes(base)) return base;
  for (let i = 2; ; i++) {
    const name = `${base}-${i}`;
    if (!existing.includes(name)) return name;
  }
}

/** Same rule as the backend: letters, digits, `.`, `_`, `-`; not starting with `-` / `.`. */
export const validRemoteName = (name: string) =>
  /^[A-Za-z0-9_][A-Za-z0-9._-]{0,63}$/.test(name) && !name.endsWith(".lock");

/** URL in the transport of `like` (SSH when `like` is SSH). */
export function urlLike(like: string | null | undefined, https: string, ssh: string): string {
  const u = (like ?? "").trim().toLowerCase();
  const isSsh = u.startsWith("ssh://") || /^[\w.-]+@[\w.-]+:/.test(u);
  if (isSsh && ssh) return ssh;
  return https || ssh;
}

/** Plain-language text (and fix button) for a failed fork command. */
export function describeForkFailure(
  t: TFunction,
  outcome: Exclude<ForkOutcome<unknown>, { status: "done" }>,
  accountId: string,
  account: string,
  locale = "vi",
): DescribedProblem {
  if (outcome.status === "noToken") {
    return {
      text: t("forks.error.noToken", { account }),
      action: {
        label: t("ci.action.reauth"),
        run: () => useAccountDialogs.getState().openManager({ accountId, reauth: true }),
      },
    };
  }
  switch (outcome.problem) {
    case "forkingDisabled":
      return { text: t("forks.error.forkingDisabled"), action: null };
    case "nameTaken":
      return { text: t("forks.error.nameTaken"), action: null };
    case "ownRepository":
      return { text: t("forks.error.own"), action: null };
    default:
      return describeProviderError(t, outcome.error, accountId, account, locale);
  }
}

export type ForkPhase = "forking" | "waiting" | "cloning" | "linking" | "fetching";
