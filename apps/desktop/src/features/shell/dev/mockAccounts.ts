/**
 * Dev-only fake accounts for the web mock, added with `&accounts=gitlab`:
 * `http://localhost:1437/?mock=shell&lang=vi&accounts=gitlab`.
 *
 * - `backend-api` (tab 1) is bound to the GitLab account, but `main` tracks `origin` on
 *   github.com (locked to the GitHub account): the account pill warns and Push/Pull ask
 *   before going through `origin`. "Pull/Push qua gitlab" moves `main` to the `gitlab` remote.
 * - `game-assets` (tab 2) is unbound with a gitlab.com remote. With `&signedOut` there is
 *   no GitLab account yet: the banner offers "Đăng nhập GitLab", which opens the wizard and
 *   starts the browser sign-in.
 * - `&wrongAccount`: `origin` (github.com) of tab 1 is bound to the GitLab account, so Push is
 *   refused by the credential helper and the notice offers the ways out.
 */
import type {
  Account,
  Identity,
  Provider,
  RemoteAccountContext,
  RepoAccountContext,
  RepoBinding,
  RepoRefs,
} from "@/ipc";

type Handler = (args: Record<string, unknown>) => unknown;

const provider = (over: Partial<Provider> & Pick<Provider, "id" | "kind" | "name">): Provider => ({
  baseUrl: "https://github.com",
  apiUrl: "https://api.github.com",
  oauthClientId: null,
  largeFileWarnMb: 50,
  largeFileBlockMb: 100,
  builtin: true,
  ...over,
});

const PROVIDERS: Provider[] = [
  provider({ id: "github", kind: "github", name: "GitHub" }),
  provider({
    id: "gitlab",
    kind: "gitlab",
    name: "GitLab",
    baseUrl: "https://gitlab.com",
    apiUrl: "https://gitlab.com/api/v4",
    largeFileBlockMb: null,
  }),
];

const account = (over: Partial<Account> & Pick<Account, "id" | "providerId">): Account => ({
  username: over.id,
  displayName: null,
  avatarUrl: null,
  authMethod: "oauth",
  scopes: [],
  tokenExpiresAt: null,
  color: "#1c7ed6",
  label: null,
  defaultCloneDir: null,
  secrets: ["token"],
  sortOrder: 0,
  createdAt: 1,
  updatedAt: 1,
  ...over,
});

const PRET = account({ id: "acc-pret", providerId: "github", username: "pret", label: "Pret" });
const CARTIMES = account({
  id: "acc-cartimes",
  providerId: "gitlab",
  username: "cartimes_cs",
  label: "Cartimes CS Gitlab",
  color: "#0c8599",
  sortOrder: 1,
});
const query = new URLSearchParams(location.search);
const ACCOUNTS = query.has("signedOut") ? [PRET] : [PRET, CARTIMES];
/** Account of `origin` in tab 1 (changed by binding that remote). */
let originAccount = query.has("wrongAccount") ? CARTIMES : PRET;

const identity = (id: string, a: Account, email: string): Identity => ({
  id,
  accountId: a.id,
  name: a.label ?? a.username,
  email,
  signing: "none",
  signingKeyRef: null,
  isDefault: true,
  createdAt: 1,
});
const IDENTITIES = [
  identity("idn-pret", PRET, "pret@example.dev"),
  identity("idn-cartimes", CARTIMES, "cs@cartimes.example"),
];
const identityOf = (a: Account | null) => IDENTITIES.find((i) => i.accountId === a?.id) ?? null;

const remote = (
  name: string,
  url: string,
  a: Account | null,
  over: Partial<RemoteAccountContext> = {},
): RemoteAccountContext => ({
  remote: name,
  url,
  host: new URL(url).host,
  transport: "https",
  account: a,
  identity: identityOf(a),
  source: a ? "manual" : "none",
  matchedRule: null,
  inherited: true,
  ...over,
});

const binding = (over: Partial<RepoBinding> & Pick<RepoBinding, "repoId" | "accountId">) => ({
  remoteName: null,
  identityId: null,
  transport: null,
  source: "manual" as const,
  locked: false,
  updatedAt: 1,
  ...over,
});

/** Repo-level account of each fake repo (changed by `repo_binding_set`). */
const repoAccount: Record<number, Account | null> = { 1: CARTIMES, 2: null, 3: PRET };
/** Remote `main` of tab 1 tracks (changed by `branch_track_remote`). */
let tracked = "origin";

function context(repoId: number): RepoAccountContext {
  const a = repoAccount[repoId] ?? null;
  const perRemote =
    repoId === 1
      ? [
          remote("origin", "https://github.com/acme/backend-api.git", originAccount, {
            source: "locked",
            inherited: false,
          }),
          remote("gitlab", "https://gitlab.com/acme/backend-api.git", CARTIMES, {
            inherited: a?.id === CARTIMES.id,
          }),
        ]
      : repoId === 2
        ? [remote("origin", "https://gitlab.com/acme/game-assets.git", a)]
        : [remote("origin", "https://github.com/acme/infra.git", a)];
  return {
    repoId,
    account: a,
    identity: identityOf(a),
    source: a ? "manual" : "none",
    matchedRule: null,
    perRemote,
    bindings: a ? [binding({ repoId, accountId: a.id })] : [],
  };
}

const done = (value: unknown) => ({ status: "done", value });

/** Account commands of the scenario; `base` runs the shell mock's own handler. */
export function accountHandlers(base: (cmd: string, args: Record<string, unknown>) => unknown) {
  const handlers: Record<string, Handler> = {
    provider_list: () => PROVIDERS,
    account_list: () => ACCOUNTS,
    identity_list: (a) => IDENTITIES.filter((i) => !a.accountId || i.accountId === a.accountId),
    account_health_all: () => [],
    repo_account_context: (a) => context(a.repoId as number),
    repo_binding_set: (a) => {
      const input = a.binding as { accountId: string; remote: string | null };
      if (a.repoId === 1 && input.remote === "origin") {
        originAccount = ACCOUNTS.find((x) => x.id === input.accountId) ?? originAccount;
      }
      if (!input.remote) {
        repoAccount[a.repoId as number] = ACCOUNTS.find((x) => x.id === input.accountId) ?? null;
      }
      return context(a.repoId as number);
    },
    repo_binding_clear: (a) => {
      repoAccount[a.repoId as number] = null;
      return context(a.repoId as number);
    },
    repo_refs: (a) => {
      const refs = base("repo_refs", a) as RepoRefs;
      if (a.id !== 1) return refs;
      return {
        ...refs,
        local: refs.local.map((b) =>
          b.name === "main" && tracked !== "origin"
            ? { ...b, upstream: `${tracked}/main`, ahead: 0, behind: 0, upstreamGone: true }
            : b,
        ),
        remotes: [
          ...refs.remotes,
          { name: "gitlab", url: "https://gitlab.com/acme/backend-api.git", branches: [] },
        ],
      };
    },
    branch_track_remote: (a) => {
      tracked = a.remote as string;
      return done(null);
    },
    push_guard: () => ({ status: "cancelled" }),
    remote_fetch: () => done({ updatedRefs: 0 }),
    remote_pull: () => done({ result: "upToDate", conflicts: [], pending: null, head: null }),
    remote_push: (a) => {
      const remoteName = (a.request as { remote: string }).remote;
      if (a.repoId === 1 && remoteName === "origin" && originAccount !== PRET) {
        return {
          status: "rejected",
          reason: "accountHostMismatch",
          message: [
            "nanomgg-credential: host `github.com` does not belong to this account (expected `gitlab.com`)",
            "fatal: credential helper told us to quit",
          ].join(String.fromCharCode(10)),
        };
      }
      return done({ upToDate: false });
    },
    auth_config: (a) => ({
      providerId: a.providerId,
      hasClientId: true,
      builtinClientId: true,
      deviceFlow: true,
      pkce: a.providerId === "gitlab",
      loopbackPort: 47821,
      redirectUri: "http://127.0.0.1:47821/callback",
      patUrl: "https://gitlab.com/-/user_settings/personal_access_tokens",
      patScopes: ["read_user", "api", "read_repository", "write_repository"],
      oauthAppsUrl: "https://gitlab.com/-/user_settings/applications",
    }),
    auth_pkce_start: () => ({
      flowId: "flow-1",
      authorizeUrl: "https://gitlab.com/oauth/authorize?client_id=fake",
      redirectUri: "http://127.0.0.1:47821/callback",
    }),
    // "The browser answers" after a moment: the GitLab account appears and, as the resolver
    // does for the only account on a host, the gitlab.com repo uses it.
    auth_pkce_wait: () =>
      new Promise((resolve) =>
        setTimeout(() => {
          if (!ACCOUNTS.includes(CARTIMES)) ACCOUNTS.push(CARTIMES);
          repoAccount[2] = CARTIMES;
          resolve(CARTIMES);
        }, 1500),
      ),
    auth_cancel: () => true,
  };
  return handlers;
}
