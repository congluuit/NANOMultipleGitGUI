/**
 * Selection logic of the import wizard (pure, unit-tested): which suggestions start ticked,
 * which account each one goes to, and the `ImportSelection` sent to `import_apply`.
 */
import type {
  Account,
  ImportProviderKind,
  ImportScan,
  ImportSelection,
  Provider,
  SourceRef,
  SuggestedIdentity,
  SuggestedRule,
} from "@/ipc";

/** One private key file referenced by `IdentityFile`, merged over every host using it. */
export interface KeyCandidate {
  path: string;
  /** Host aliases that use the key (`github.com-work`). */
  aliases: { alias: string; hostName: string; isAlias: boolean }[];
  /** Real host of the first alias (`github.com`). */
  hostName: string;
  provider: ImportProviderKind;
  exists: boolean;
  /** Already imported (same fingerprint). */
  known: boolean;
  fingerprint: string | null;
  comment: string | null;
  source: SourceRef;
}

export interface Choice {
  checked: boolean;
  accountId: string | null;
}

export interface IdentityChoiceState extends Choice {
  makeDefault: boolean;
}

export interface KeyChoiceState extends Choice {
  passphrase: string;
}

/** Index-aligned with `report.identities`, `report.rules` and {@link keyCandidates}. */
export interface WizardState {
  identities: IdentityChoiceState[];
  rules: Choice[];
  keys: KeyChoiceState[];
}

const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();

/** Keys of non-wildcard ssh hosts, one entry per file. */
export function keyCandidates(scan: ImportScan): KeyCandidate[] {
  const known = new Set(scan.knownFingerprints);
  const byPath = new Map<string, KeyCandidate>();
  for (const host of scan.report.sshHosts) {
    if (host.wildcard) continue;
    for (const file of host.identityFiles) {
      const key = norm(file.path);
      const entry = { alias: host.alias, hostName: host.hostName, isAlias: host.isAlias };
      const existing = byPath.get(key);
      if (existing) {
        existing.aliases.push(entry);
        continue;
      }
      byPath.set(key, {
        path: file.path,
        aliases: [entry],
        hostName: host.hostName,
        provider: host.provider,
        exists: file.exists,
        known: !!file.fingerprint && known.has(file.fingerprint),
        fingerprint: file.fingerprint,
        comment: file.comment,
        source: file.source,
      });
    }
  }
  return [...byPath.values()];
}

/** Host name of a provider (`https://gitlab.acme.dev:8443` → `gitlab.acme.dev`). */
export function providerHost(provider: Pick<Provider, "baseUrl">): string | null {
  try {
    return new URL(provider.baseUrl).hostname.toLowerCase();
  } catch {
    return null;
  }
}

/**
 * Best account for a suggestion: the only account on `host` (or the one whose user name
 * matches), else the only account at all. `null` when ambiguous.
 */
export function guessAccount(
  accounts: readonly Account[],
  providers: readonly Provider[],
  host?: string | null,
  username?: string | null,
): string | null {
  if (host) {
    const h = host.toLowerCase();
    const onHost = accounts.filter((a) => {
      const p = providers.find((p) => p.id === a.providerId);
      return p && providerHost(p) === h;
    });
    if (username) {
      const named = onHost.find((a) => a.username.toLowerCase() === username.toLowerCase());
      if (named) return named.id;
    }
    if (onHost.length > 1) return null;
    if (onHost[0]) return onHost[0].id;
  }
  return accounts.length === 1 && accounts[0] ? accounts[0].id : null;
}

const hasCondition = (r: SuggestedRule) =>
  !!(r.pathGlob || r.host || r.ownerOrGroup || r.remoteRegex);

/** The `IdentityFile` host an identity's `core.sshCommand` key points at, if any. */
function identityHost(identity: SuggestedIdentity, keys: readonly KeyCandidate[]) {
  if (!identity.sshKeyPath) return null;
  const k = keys.find((c) => norm(c.path) === norm(identity.sshKeyPath!));
  return k?.hostName ?? null;
}

export function initialState(
  scan: ImportScan,
  accounts: readonly Account[],
  providers: readonly Provider[],
): WizardState {
  const keys = keyCandidates(scan);
  const identities = scan.report.identities.map((i): IdentityChoiceState => {
    const accountId = guessAccount(accounts, providers, identityHost(i, keys));
    return { checked: !!i.email, accountId, makeDefault: false };
  });
  const rules = scan.report.rules.map((r): Choice => {
    const fromIdentity = r.identityIndex != null ? identities[r.identityIndex] : undefined;
    return {
      checked: hasCondition(r),
      accountId: fromIdentity?.accountId ?? guessAccount(accounts, providers, r.host),
    };
  });
  const keyChoices = keys.map((k): KeyChoiceState => ({
    checked: k.exists && !k.known,
    accountId: guessAccount(accounts, providers, k.hostName),
    passphrase: "",
  }));
  return { identities, rules, keys: keyChoices };
}

/** Fills accounts that are still unset (e.g. after the user signed in from the wizard). */
export function fillMissingAccounts(
  state: WizardState,
  scan: ImportScan,
  accounts: readonly Account[],
  providers: readonly Provider[],
): WizardState {
  const fresh = initialState(scan, accounts, providers);
  const fill = <T extends Choice>(list: T[], guesses: Choice[]) =>
    list.map((c, i) => (c.accountId ? c : { ...c, accountId: guesses[i]?.accountId ?? null }));
  return {
    identities: fill(state.identities, fresh.identities),
    rules: fill(state.rules, fresh.rules),
    keys: fill(state.keys, fresh.keys),
  };
}

/** Changes an identity's account; rules using that identity follow when they had the same one. */
export function setIdentityAccount(
  state: WizardState,
  scan: ImportScan,
  index: number,
  accountId: string | null,
): WizardState {
  const previous = state.identities[index]?.accountId ?? null;
  return {
    ...state,
    identities: state.identities.map((c, i) => (i === index ? { ...c, accountId } : c)),
    rules: state.rules.map((c, i) =>
      scan.report.rules[i]?.identityIndex === index &&
      (c.accountId === previous || c.accountId === null)
        ? { ...c, accountId }
        : c,
    ),
  };
}

/** Accounts are required for identities and rules; keys may stay unassigned. */
export const needsAccount = (kind: "identity" | "rule" | "key", c: Choice) =>
  kind !== "key" && c.checked && !c.accountId;

/** gh/glab accounts not added yet (matched by user name): they need the sign-in flow. */
export function pendingSignIns(scan: ImportScan, accounts: readonly Account[]) {
  return scan.report.accounts
    .filter(
      (a) =>
        !a.username ||
        !accounts.some((acc) => acc.username.toLowerCase() === a.username!.toLowerCase()),
    )
    .map((a) => ({ host: a.host, username: a.username }));
}

export function buildSelection(
  scan: ImportScan,
  state: WizardState,
  accounts: readonly Account[] = [],
): ImportSelection {
  const { identities, rules } = scan.report;
  const keys = keyCandidates(scan);
  return {
    identities: identities.flatMap((i, idx) => {
      const c = state.identities[idx];
      if (!c?.checked || !c.accountId || !i.email) return [];
      return [
        {
          accountId: c.accountId,
          name: i.name || i.email,
          email: i.email,
          makeDefault: c.makeDefault,
        },
      ];
    }),
    rules: rules.flatMap((r, idx) => {
      const c = state.rules[idx];
      if (!c?.checked || !c.accountId || !hasCondition(r)) return [];
      const ident = r.identityIndex != null ? identities[r.identityIndex] : undefined;
      const identChoice = r.identityIndex != null ? state.identities[r.identityIndex] : undefined;
      const identityEmail =
        ident?.email && identChoice?.checked && identChoice.accountId === c.accountId
          ? ident.email
          : null;
      // Host/owner pinned by the URL glob are clearer than its regex; keep the regex otherwise.
      const pinned = !!(r.host || r.ownerOrGroup);
      return [
        {
          accountId: c.accountId,
          pathGlob: r.pathGlob,
          host: r.host,
          ownerOrGroup: r.ownerOrGroup,
          remoteRegex: pinned ? null : r.remoteRegex,
          identityEmail,
        },
      ];
    }),
    sshKeys: keys.flatMap((k, idx) => {
      const c = state.keys[idx];
      if (!c?.checked || !k.exists) return [];
      return [{ path: k.path, accountId: c.accountId, passphrase: c.passphrase || null }];
    }),
    accounts: pendingSignIns(scan, accounts),
    markDone: true,
  };
}

export const selectionSize = (s: ImportSelection) =>
  (s.identities?.length ?? 0) + (s.rules?.length ?? 0) + (s.sshKeys?.length ?? 0);

/** Nothing worth showing on the "choose" step. */
export const isEmptyScan = (scan: ImportScan) =>
  scan.report.identities.length === 0 &&
  scan.report.rules.length === 0 &&
  keyCandidates(scan).length === 0 &&
  scan.report.accounts.length === 0;
