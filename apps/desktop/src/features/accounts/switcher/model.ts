/** Pure helpers of the account switcher (PLAN §4.3, §4.5). No React, no IPC. */
import type {
  Account,
  ContextSource,
  Identity,
  Provider,
  ProviderKind,
  RemoteAccountContext,
  RepoAccountContext,
  Rule,
  Transport,
} from "@/ipc";

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
