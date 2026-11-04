/** Pure helpers of the account switcher (PLAN §4.3, §4.5). No React, no IPC. */
import type {
  Account,
  ContextSource,
  Identity,
  Provider,
  ProviderKind,
  RemoteAccountContext,
  RepoAccountContext,
  RepoRefs,
  Rule,
  Transport,
} from "@/ipc";
import { pushPlan } from "@/features/remote/plan";
import type { SignInTarget } from "../dialogs";

/** Fields of an account needed to name it. */
export type NamedAccount = Pick<Account, "label" | "displayName" | "username">;

/** "Công ty" → user label, else the display name, else the username. */
export const accountLabel = (a: NamedAccount) =>
  a.label?.trim() || a.displayName?.trim() || a.username;

/** First letter (Unicode aware) of `text`, upper-cased; `?` for an empty string. */
export const initialOf = (text: string) => {
  const first = [...text.trim()][0];
  return first ? first.toLocaleUpperCase() : "?";
};

/** Switcher order: `sortOrder` (the `Ctrl/Cmd+Alt+N` position), then creation time. */
export const sortAccounts = (accounts: readonly Account[]): Account[] =>
  [...accounts].sort((a, b) => a.sortOrder - b.sortOrder || a.createdAt - b.createdAt);

/** Identities of one account, the default one first. */
export const identitiesOf = (identities: readonly Identity[], accountId: string): Identity[] =>
  identities
    .filter((i) => i.accountId === accountId)
    .sort((a, b) => Number(b.isDefault) - Number(a.isDefault) || a.createdAt - b.createdAt);

/** The identity a binding without `identityId` gets: the default one, else the first. */
export const defaultIdentity = (identities: readonly Identity[], accountId: string) =>
  identitiesOf(identities, accountId)[0] ?? null;

const KIND_NAMES: Record<ProviderKind, string> = {
  github: "GitHub",
  githubEnterprise: "GitHub Enterprise",
  gitlab: "GitLab",
  gitlabSelfHosted: "GitLab",
};

/** Name of the provider of `account` ("GitLab acme"), `null` while providers are unknown. */
export function providerNameOf(
  account: Pick<Account, "providerId">,
  providers: readonly Provider[] | undefined,
): string | null {
  const p = providers?.find((x) => x.id === account.providerId);
  return p ? p.name || KIND_NAMES[p.kind] : null;
}

/** Short text for a rule: `github.com/acme-corp · D:/Work/** · /regex/`. */
export function ruleSummary(rule: Rule): string {
  const parts: string[] = [];
  const remote = [rule.host, rule.ownerOrGroup].filter(Boolean).join("/");
  if (remote) parts.push(rule.ownerOrGroup ? `${remote}/*` : remote);
  if (rule.pathGlob) parts.push(rule.pathGlob);
  if (rule.remoteRegex) parts.push(`/${rule.remoteRegex}/`);
  return parts.join(" · ") || "*";
}

/** `origin` when present, else the first remote. */
export const primaryRemoteContext = (
  perRemote: readonly RemoteAccountContext[],
): RemoteAccountContext | null =>
  perRemote.find((r) => r.remote === "origin") ?? perRemote[0] ?? null;

/**
 * Transport git will use with the bound account: the one implied by the primary remote's URL,
 * else the preferred transport of the repo-level binding (repos without remotes).
 */
export function contextTransport(ctx: RepoAccountContext): Transport | null {
  const primary = primaryRemoteContext(ctx.perRemote);
  if (primary?.transport) return primary.transport;
  return ctx.bindings.find((b) => b.remoteName === null)?.transport ?? null;
}

/** Some remote is bound on its own or resolves to another account than the repo. */
export const hasSplitRemotes = (ctx: RepoAccountContext) =>
  ctx.perRemote.some((r) => !r.inherited || (r.account?.id ?? null) !== (ctx.account?.id ?? null));

/** A repo-level binding exists, so "Bỏ gắn" has something to remove. */
export const hasRepoBinding = (ctx: RepoAccountContext | undefined) =>
  !!ctx?.bindings.some((b) => b.remoteName === null);

/** What the UI shows for a repo's account context (pill, border, terminal, "Commit as"). */
export interface RepoAccount {
  context: RepoAccountContext | null;
  account: Account | null;
  identity: Identity | null;
  source: ContextSource;
  matchedRule: Rule | null;
  /** Account color (`#rrggbb`), `null` when unbound. */
  color: string | null;
  label: string | null;
  providerName: string | null;
  transport: Transport | null;
}

export function describeRepoAccount(
  context: RepoAccountContext | null | undefined,
  providers: readonly Provider[] | undefined,
): RepoAccount {
  const account = context?.account ?? null;
  return {
    context: context ?? null,
    account,
    identity: account ? (context?.identity ?? null) : null,
    source: context?.source ?? "none",
    matchedRule: context?.matchedRule ?? null,
    color: account?.color ?? null,
    label: account ? accountLabel(account) : null,
    providerName: account ? providerNameOf(account, providers) : null,
    transport: context ? contextTransport(context) : null,
  };
}

/**
 * Flat account info with the fields the old sample accounts had (`label`, `color`,
 * `providerName`, `identity.email`), for one-line swaps of `findAccount(...)`.
 */
export interface AccountSummary {
  id: string;
  label: string;
  color: string;
  /** "GitLab acme"; empty while providers are loading. */
  providerName: string;
  username: string;
  avatarUrl: string | null;
  identity: { name: string; email: string } | null;
  transport: Transport | null;
}

export function toAccountSummary(r: RepoAccount): AccountSummary | null {
  const a = r.account;
  if (!a) return null;
  return {
    id: a.id,
    label: r.label ?? accountLabel(a),
    color: a.color,
    providerName: r.providerName ?? "",
    username: a.username,
    avatarUrl: a.avatarUrl,
    identity: r.identity ? { name: r.identity.name, email: r.identity.email } : null,
    transport: r.transport,
  };
}

/** Modifier/key fields of a keyboard event used by the switcher shortcut. */
export interface ShortcutKey {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  /** AltGr on Windows reports Ctrl+Alt; it types characters (`{`, `@`…) and must not switch. */
  altGraph?: boolean;
}

/**
 * `Ctrl+Alt+1..9` (`⌘+⌥+1..9` on macOS) → account index 0..8, else `null`. Uses the physical key
 * (`Digit1`, `Numpad1`) so it works on any keyboard layout.
 */
export function accountShortcutIndex(e: ShortcutKey, isMac: boolean): number | null {
  const mods = isMac ? e.metaKey && e.altKey && !e.ctrlKey : e.ctrlKey && e.altKey && !e.metaKey;
  if (!mods || e.shiftKey || e.altGraph) return null;
  const m = /^(?:Digit|Numpad)([1-9])$/.exec(e.code);
  return m ? Number(m[1]) - 1 : null;
}

/** Shortcut hint of the account at `index` (0-based), `null` past the ninth. */
export const accountShortcutHint = (index: number, isMac: boolean) =>
  index < 9 ? `${isMac ? "⌘+⌥" : "Ctrl+Alt"}+${index + 1}` : null;

/**
 * The current branch pulls from / pushes to a remote that runs as another account than the
 * repo's, while another remote runs as the repo's account: e.g. the pill says "Company GitLab"
 * but `main` tracks `origin` on github.com, so Push would still go to GitHub as a GitHub account.
 */
export interface TrackingMismatch {
  branch: string;
  /** Where Pull/Push go now. */
  current: RemoteAccountContext;
  /** A remote that uses the repo's account. */
  target: RemoteAccountContext;
}

export function trackingMismatch(
  ctx: RepoAccountContext | null | undefined,
  refs: RepoRefs | null | undefined,
): TrackingMismatch | null {
  const accountId = ctx?.account?.id;
  const plan = refs ? pushPlan(refs) : null;
  if (!ctx || !accountId || !plan) return null;
  const current = ctx.perRemote.find((r) => r.remote === plan.remote);
  if (!current || current.account?.id === accountId) return null;
  const target = ctx.perRemote.find((r) => r.remote !== plan.remote && r.account?.id === accountId);
  return target ? { branch: plan.branch, current, target } : null;
}

const hostOfUrl = (url: string) => {
  try {
    return new URL(url).host.toLowerCase();
  } catch {
    return "";
  }
};

/**
 * Provider to sign in to for a remote's host: a known instance, github.com, gitlab.com, or a
 * self-hosted GitLab when the host name says so. `null` when it cannot be told.
 */
export function signInTargetFor(
  remote: Pick<RemoteAccountContext, "host" | "transport">,
  providers: readonly Provider[],
): SignInTarget | null {
  let host = remote.host?.toLowerCase() ?? "";
  // An SSH port says nothing about the web address.
  if (remote.transport === "ssh") host = host.replace(/:\d+$/, "");
  if (!host) return null;
  const known = providers.find((p) => hostOfUrl(p.baseUrl) === host);
  if (known) return { kind: known.kind, baseUrl: known.baseUrl };
  if (host === "github.com") return { kind: "github", baseUrl: "https://github.com" };
  if (host === "gitlab.com") return { kind: "gitlab", baseUrl: "https://gitlab.com" };
  if (/(^|[.-])gitlab([.-]|$)/.test(host)) {
    return { kind: "gitlabSelfHosted", baseUrl: `https://${host}` };
  }
  return null;
}

/** "GitLab", or the name of the known instance at that address. */
export function signInName(target: SignInTarget, providers: readonly Provider[]): string {
  const known = providers.find((p) => p.kind === target.kind && p.baseUrl === target.baseUrl);
  return known?.name || KIND_NAMES[target.kind];
}

/**
 * First remote (primary first) that no account can serve because nobody is signed in to its
 * host: what "Sign in to GitLab" should open. `null` when every remote has an account.
 */
export function missingSignIn(
  ctx: RepoAccountContext | null | undefined,
  accounts: readonly Account[],
  providers: readonly Provider[],
): SignInTarget | null {
  if (!ctx) return null;
  const primary = primaryRemoteContext(ctx.perRemote);
  const ordered = primary ? [primary, ...ctx.perRemote.filter((r) => r !== primary)] : [];
  for (const remote of ordered) {
    if (remote.account) continue;
    const target = signInTargetFor(remote, providers);
    if (!target) continue;
    const host = hostOfUrl(target.baseUrl);
    const signedIn = accounts.some((a) => {
      const p = providers.find((x) => x.id === a.providerId);
      return p && hostOfUrl(p.baseUrl) === host;
    });
    if (!signedIn) return target;
  }
  return null;
}

const bareHost = (host: string) => host.toLowerCase().replace(/:\d+$/, "");

/** `gitlab.com` for an account of the gitlab.com provider; `null` while providers load. */
export function accountHostOf(
  account: Pick<Account, "providerId">,
  providers: readonly Provider[],
): string | null {
  const p = providers.find((x) => x.id === account.providerId);
  const host = p ? hostOfUrl(p.baseUrl) : "";
  return host ? bareHost(host) : null;
}

/**
 * Whether `account` can sign in to `remote` over HTTPS: the app only sends a token to its own
 * provider's server. SSH remotes and unknown providers are not judged (host aliases exist).
 */
export function accountFitsRemote(
  account: Pick<Account, "providerId">,
  remote: Pick<RemoteAccountContext, "host" | "transport">,
  providers: readonly Provider[],
): boolean {
  if (remote.transport !== "https" || !remote.host) return true;
  const host = accountHostOf(account, providers);
  return host === null || host === bareHost(remote.host);
}

/**
 * Ways out when a push/fetch was refused because a remote is bound to an account of another
 * server (`accountHostMismatch`, e.g. a github.com remote bound to a GitLab account).
 */
export interface HostMismatchFix {
  /** The remote on the refused server. */
  remote: RemoteAccountContext;
  /** Server of the account bound to it (`gitlab.com`). */
  accountHost: string | null;
  /** Another remote on the bound account's own server: where the push was probably meant to go. */
  target: RemoteAccountContext | null;
  /** Accounts signed in to the refused server. */
  candidates: Account[];
  /** Nobody is signed in there: where "Sign in" should open. */
  signIn: SignInTarget | null;
}

/** Remote on the refused server in a credential helper error ("host `github.com` does not…"). */
export const refusedHostOf = (text: string) =>
  /host `([^`]+)` does not belong to this account/i.exec(text)?.[1] ?? null;

export function hostMismatchFix(
  ctx: RepoAccountContext | null | undefined,
  accounts: readonly Account[],
  providers: readonly Provider[],
  refusedHost: string,
): HostMismatchFix | null {
  if (!ctx) return null;
  const want = bareHost(refusedHost);
  const onHost = ctx.perRemote.filter((r) => r.host && bareHost(r.host) === want);
  const remote = onHost.find((r) => r.account) ?? onHost[0];
  if (!remote) return null;
  const wrong = remote.account;
  const accountHost = wrong ? accountHostOf(wrong, providers) : null;
  const target =
    (wrong &&
      accountHost &&
      ctx.perRemote.find(
        (r) =>
          r !== remote &&
          r.account?.id === wrong.id &&
          !!r.host &&
          bareHost(r.host) === accountHost,
      )) ||
    null;
  const candidates = sortAccounts(accounts.filter((a) => accountHostOf(a, providers) === want));
  return {
    remote,
    accountHost,
    target,
    candidates,
    signIn: candidates.length ? null : signInTargetFor(remote, providers),
  };
}
