/** Fixtures for tests that need account data in the TanStack Query cache. Test-only. */
import type { QueryClient } from "@tanstack/react-query";
import type {
  Account,
  Identity,
  Provider,
  RemoteAccountContext,
  RepoAccountContext,
  RepoBinding,
  Rule,
} from "@/ipc";
import { accountKeys } from "../queries";

export const PROVIDERS: Provider[] = [
  {
    id: "gh",
    kind: "github",
    name: "GitHub",
    baseUrl: "https://github.com",
    apiUrl: "https://api.github.com",
    oauthClientId: null,
    largeFileWarnMb: 50,
    largeFileBlockMb: 100,
    builtin: true,
  },
  {
    id: "gl-acme",
    kind: "gitlabSelfHosted",
    name: "GitLab acme",
    baseUrl: "https://gitlab.acme.dev",
    apiUrl: "https://gitlab.acme.dev/api/v4",
    oauthClientId: null,
    largeFileWarnMb: 50,
    largeFileBlockMb: null,
    builtin: false,
  },
];

export function makeAccount(over: Partial<Account> & { id: string }): Account {
  return {
    providerId: "gh",
    username: over.id,
    displayName: null,
    avatarUrl: null,
    authMethod: "pat",
    scopes: [],
    tokenExpiresAt: null,
    color: "#5c7cfa",
    label: null,
    defaultCloneDir: null,
    secrets: ["token"],
    sortOrder: 0,
    createdAt: 1,
    updatedAt: 1,
    ...over,
  };
}

export function makeIdentity(
  over: Partial<Identity> & { id: string; accountId: string },
): Identity {
  return {
    name: "Hoang",
    email: `${over.id}@example.dev`,
    signing: "none",
    signingKeyRef: null,
    isDefault: false,
    createdAt: 1,
    ...over,
  };
}

/** Work (GitLab acme, order 0), Personal (GitHub, order 1, two identities), OSS (order 2). */
export const WORK = makeAccount({
  id: "acc-work",
  providerId: "gl-acme",
  username: "hoang.nv",
  label: "Công ty",
  color: "#5c7cfa",
  sortOrder: 0,
});
export const PERSONAL = makeAccount({
  id: "acc-personal",
  username: "hoangnv",
  label: "Cá nhân",
  color: "#e64980",
  sortOrder: 1,
});
export const OSS = makeAccount({
  id: "acc-oss",
  username: "hoangnv-oss",
  displayName: "Hoang OSS",
  color: "#be4bdb",
  sortOrder: 2,
});
export const ACCOUNTS = [OSS, WORK, PERSONAL]; // unsorted on purpose

export const WORK_ID = makeIdentity({
  id: "id-work",
  accountId: WORK.id,
  name: "Hoang Nguyen",
  email: "hoang@acme.dev",
  isDefault: true,
});
export const PERSONAL_MAIN = makeIdentity({
  id: "id-personal",
  accountId: PERSONAL.id,
  name: "Hoang NV",
  email: "hoangnv@users.noreply.github.com",
  isDefault: true,
});
export const PERSONAL_ALT = makeIdentity({
  id: "id-personal-alt",
  accountId: PERSONAL.id,
  name: "Hoang (blog)",
  email: "blog@hoangnv.dev",
  createdAt: 2,
});
export const IDENTITIES = [WORK_ID, PERSONAL_MAIN, PERSONAL_ALT];

export function makeRule(over: Partial<Rule> = {}): Rule {
  return {
    id: "rule-1",
    priority: 0,
    host: "gitlab.acme.dev",
    ownerOrGroup: "platform",
    pathGlob: null,
    remoteRegex: null,
    accountId: WORK.id,
    identityId: null,
    enabled: true,
    createdAt: 1,
    ...over,
  };
}

export function makeBinding(over: Partial<RepoBinding> = {}): RepoBinding {
  return {
    repoId: 1,
    remoteName: null,
    accountId: WORK.id,
    identityId: null,
    transport: null,
    source: "manual",
    locked: false,
    updatedAt: 1,
    ...over,
  };
}

export function makeRemote(over: Partial<RemoteAccountContext> = {}): RemoteAccountContext {
  return {
    remote: "origin",
    url: "https://gitlab.acme.dev/platform/api.git",
    host: "gitlab.acme.dev",
    transport: "https",
    account: WORK,
    identity: WORK_ID,
    source: "manual",
    matchedRule: null,
    inherited: true,
    ...over,
  };
}

/** Context of repo `repoId` bound to Work (manual) with an `origin` over HTTPS. */
export function makeContext(over: Partial<RepoAccountContext> = {}): RepoAccountContext {
  return {
    repoId: 1,
    account: WORK,
    identity: WORK_ID,
    source: "manual",
    matchedRule: null,
    perRemote: [makeRemote()],
    bindings: [makeBinding()],
    ...over,
  };
}

/** Unbound context (rule engine found nothing). */
export const unboundContext = (repoId = 1): RepoAccountContext => ({
  repoId,
  account: null,
  identity: null,
  source: "none",
  matchedRule: null,
  perRemote: [makeRemote({ account: null, identity: null, source: "none" })],
  bindings: [],
});

/** Seeds providers, accounts, identities and (optionally) per-repo contexts. */
export function seedAccounts(
  client: QueryClient,
  opts: {
    accounts?: Account[];
    identities?: Identity[];
    contexts?: RepoAccountContext[];
  } = {},
) {
  client.setQueryData(accountKeys.providers, PROVIDERS);
  client.setQueryData(accountKeys.list, opts.accounts ?? ACCOUNTS);
  client.setQueryData(accountKeys.identities(null), opts.identities ?? IDENTITIES);
  for (const ctx of opts.contexts ?? []) {
    client.setQueryData(accountKeys.repoContext(ctx.repoId), ctx);
  }
}
