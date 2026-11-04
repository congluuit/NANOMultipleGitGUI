import { describe, expect, it } from "vitest";
import type { LocalBranch, RepoRefs } from "@/ipc";
import {
  accountLabel,
  accountShortcutHint,
  accountShortcutIndex,
  contextTransport,
  defaultIdentity,
  describeRepoAccount,
  hasRepoBinding,
  hasSplitRemotes,
  identitiesOf,
  accountFitsRemote,
  hostMismatchFix,
  initialOf,
  missingSignIn,
  refusedHostOf,
  providerNameOf,
  ruleSummary,
  signInName,
  signInTargetFor,
  sortAccounts,
  toAccountSummary,
  trackingMismatch,
  type ShortcutKey,
} from "./model";
import {
  ACCOUNTS,
  IDENTITIES,
  OSS,
  PERSONAL,
  PERSONAL_ALT,
  PERSONAL_MAIN,
  PROVIDERS,
  WORK,
  WORK_ID,
  makeBinding,
  makeContext,
  makeRemote,
  makeRule,
  unboundContext,
} from "./testData";

describe("names", () => {
  it("labels an account by label, then display name, then username", () => {
    expect(accountLabel(WORK)).toBe("Công ty");
    expect(accountLabel(OSS)).toBe("Hoang OSS");
    expect(accountLabel({ label: "  ", displayName: null, username: "bob" })).toBe("bob");
  });

  it("takes the first (Unicode) letter as initial", () => {
    expect(initialOf("công ty")).toBe("C");
    expect(initialOf("đức")).toBe("Đ");
    expect(initialOf("  ")).toBe("?");
  });

  it("names the provider, falling back to its kind", () => {
    expect(providerNameOf(WORK, PROVIDERS)).toBe("GitLab acme");
    expect(providerNameOf(WORK, undefined)).toBeNull();
    const unnamed = [{ ...PROVIDERS[0]!, name: "" }];
    expect(providerNameOf(PERSONAL, unnamed)).toBe("GitHub");
  });
});

describe("ordering", () => {
  it("sorts accounts by sortOrder then creation", () => {
    expect(sortAccounts(ACCOUNTS).map((a) => a.id)).toEqual([WORK.id, PERSONAL.id, OSS.id]);
    const tie = [
      { ...OSS, sortOrder: 0, createdAt: 5 },
      { ...WORK, createdAt: 2 },
    ];
    expect(sortAccounts(tie).map((a) => a.id)).toEqual([WORK.id, OSS.id]);
  });

  it("lists identities of an account, default first", () => {
    const shuffled = [PERSONAL_ALT, WORK_ID, PERSONAL_MAIN];
    expect(identitiesOf(shuffled, PERSONAL.id)).toEqual([PERSONAL_MAIN, PERSONAL_ALT]);
    expect(defaultIdentity(IDENTITIES, PERSONAL.id)).toBe(PERSONAL_MAIN);
    expect(defaultIdentity(IDENTITIES, OSS.id)).toBeNull();
  });
});

describe("rules and contexts", () => {
  it("summarizes a rule", () => {
    expect(ruleSummary(makeRule())).toBe("gitlab.acme.dev/platform/*");
    expect(ruleSummary(makeRule({ ownerOrGroup: null }))).toBe("gitlab.acme.dev");
    expect(ruleSummary(makeRule({ host: null, ownerOrGroup: null, pathGlob: "D:/Work/**" }))).toBe(
      "D:/Work/**",
    );
    expect(ruleSummary(makeRule({ host: null, ownerOrGroup: null, remoteRegex: "acme" }))).toBe(
      "/acme/",
    );
  });

  it("takes the transport from origin, else the repo binding", () => {
    expect(contextTransport(makeContext())).toBe("https");
    const ssh = makeContext({
      perRemote: [
        makeRemote({ remote: "upstream", transport: "https" }),
        makeRemote({ remote: "origin", transport: "ssh" }),
      ],
    });
    expect(contextTransport(ssh)).toBe("ssh");
    const noRemote = makeContext({ perRemote: [], bindings: [makeBinding({ transport: "ssh" })] });
    expect(contextTransport(noRemote)).toBe("ssh");
    expect(contextTransport(makeContext({ perRemote: [], bindings: [] }))).toBeNull();
  });

  it("detects remotes bound differently from the repo", () => {
    expect(hasSplitRemotes(makeContext())).toBe(false);
    const own = makeContext({ perRemote: [makeRemote({ inherited: false })] });
    expect(hasSplitRemotes(own)).toBe(true);
    const ruled = makeContext({
      perRemote: [makeRemote(), makeRemote({ remote: "upstream", account: PERSONAL })],
    });
    expect(hasSplitRemotes(ruled)).toBe(true);
  });

  it("knows when there is a repo-level binding to remove", () => {
    expect(hasRepoBinding(makeContext())).toBe(true);
    expect(hasRepoBinding(unboundContext())).toBe(false);
    expect(hasRepoBinding(makeContext({ bindings: [makeBinding({ remoteName: "origin" })] }))).toBe(
      false,
    );
    expect(hasRepoBinding(undefined)).toBe(false);
  });

  it("describes a bound repo with display fields", () => {
    const r = describeRepoAccount(makeContext({ source: "locked" }), PROVIDERS);
    expect(r).toMatchObject({
      account: WORK,
      identity: WORK_ID,
      source: "locked",
      color: "#5c7cfa",
      label: "Công ty",
      providerName: "GitLab acme",
      transport: "https",
    });
    expect(toAccountSummary(r)).toEqual({
      id: WORK.id,
      label: "Công ty",
      color: "#5c7cfa",
      providerName: "GitLab acme",
      username: "hoang.nv",
      avatarUrl: null,
      identity: { name: "Hoang Nguyen", email: "hoang@acme.dev" },
      transport: "https",
    });
  });

  it("describes an unbound or unknown repo as empty", () => {
    for (const ctx of [unboundContext(), undefined]) {
      const r = describeRepoAccount(ctx, PROVIDERS);
      expect(r).toMatchObject({ account: null, color: null, label: null, source: "none" });
      expect(toAccountSummary(r)).toBeNull();
    }
    expect(describeRepoAccount(unboundContext(), PROVIDERS).context).not.toBeNull();
    expect(describeRepoAccount(undefined, PROVIDERS).context).toBeNull();
  });
});

describe("Ctrl/Cmd+Alt+1..9", () => {
  const key = (over: Partial<ShortcutKey>): ShortcutKey => ({
    code: "Digit1",
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    shiftKey: false,
    ...over,
  });

  it("maps Ctrl+Alt+N to index N-1 on Windows/Linux", () => {
    expect(accountShortcutIndex(key({ ctrlKey: true, altKey: true }), false)).toBe(0);
    expect(accountShortcutIndex(key({ code: "Digit9", ctrlKey: true, altKey: true }), false)).toBe(
      8,
    );
    expect(accountShortcutIndex(key({ code: "Numpad3", ctrlKey: true, altKey: true }), false)).toBe(
      2,
    );
  });

  it("maps Cmd+Option+N on macOS", () => {
    expect(accountShortcutIndex(key({ metaKey: true, altKey: true, code: "Digit2" }), true)).toBe(
      1,
    );
    expect(accountShortcutIndex(key({ ctrlKey: true, altKey: true }), true)).toBeNull();
  });

  it("ignores other keys and modifier sets", () => {
    const both = { ctrlKey: true, altKey: true };
    expect(accountShortcutIndex(key({ ...both, code: "Digit0" }), false)).toBeNull();
    expect(accountShortcutIndex(key({ ...both, code: "KeyA" }), false)).toBeNull();
    expect(accountShortcutIndex(key({ ...both, shiftKey: true }), false)).toBeNull();
    expect(accountShortcutIndex(key({ ...both, altGraph: true }), false)).toBeNull();
    expect(accountShortcutIndex(key({ ctrlKey: true }), false)).toBeNull();
    expect(accountShortcutIndex(key({ altKey: true }), false)).toBeNull();
    expect(accountShortcutIndex(key({ ...both, metaKey: true }), false)).toBeNull();
  });

  it("formats the hint for the first nine accounts", () => {
    expect(accountShortcutHint(0, false)).toBe("Ctrl+Alt+1");
    expect(accountShortcutHint(8, true)).toBe("⌘+⌥+9");
    expect(accountShortcutHint(9, false)).toBeNull();
  });
});

describe("tracking mismatch", () => {
  const refs = (local: Partial<LocalBranch> = {}): RepoRefs => ({
    head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
    local: [
      {
        name: "main",
        oid: "a".repeat(40),
        upstream: "origin/main",
        ahead: 1,
        behind: 0,
        upstreamGone: false,
        isHead: true,
        ...local,
      },
    ],
    remotes: [
      { name: "origin", url: "https://github.com/me/api.git", branches: [] },
      { name: "gitlab", url: "https://gitlab.acme.dev/platform/api.git", branches: [] },
    ],
    tags: [],
    stashes: [],
  });
  // Repo bound to Work (GitLab acme); `origin` is on GitHub and locked to Personal.
  const ctx = makeContext({
    perRemote: [
      makeRemote({
        remote: "origin",
        url: "https://github.com/me/api.git",
        host: "github.com",
        account: PERSONAL,
        identity: PERSONAL_MAIN,
        source: "locked",
        inherited: false,
      }),
      makeRemote({ remote: "gitlab" }),
    ],
  });

  it("finds the remote of the repo's account when the branch tracks another one", () => {
    const m = trackingMismatch(ctx, refs());
    expect(m?.branch).toBe("main");
    expect(m?.current.remote).toBe("origin");
    expect(m?.target.remote).toBe("gitlab");
  });

  it("is quiet when the branch already uses the repo's account or nothing else would", () => {
    expect(trackingMismatch(ctx, refs({ upstream: "gitlab/main" }))).toBeNull();
    const onlyOrigin = makeContext({ perRemote: [ctx.perRemote[0]!] });
    expect(trackingMismatch(onlyOrigin, refs())).toBeNull();
    expect(trackingMismatch(unboundContext(), refs())).toBeNull();
    expect(trackingMismatch(ctx, undefined)).toBeNull();
  });
});

describe("sign in for a remote", () => {
  it("maps a remote host to the provider to sign in to", () => {
    expect(signInTargetFor({ host: "gitlab.com", transport: "https" }, PROVIDERS)).toEqual({
      kind: "gitlab",
      baseUrl: "https://gitlab.com",
    });
    expect(signInTargetFor({ host: "gitlab.acme.dev:2222", transport: "ssh" }, PROVIDERS)).toEqual({
      kind: "gitlabSelfHosted",
      baseUrl: "https://gitlab.acme.dev",
    });
    expect(signInTargetFor({ host: "gitlab.corp.vn", transport: "https" }, [])).toEqual({
      kind: "gitlabSelfHosted",
      baseUrl: "https://gitlab.corp.vn",
    });
    expect(signInTargetFor({ host: "gitea.example.org", transport: "https" }, [])).toBeNull();
    expect(signInTargetFor({ host: null, transport: null }, [])).toBeNull();
  });

  it("names the provider", () => {
    expect(signInName({ kind: "gitlab", baseUrl: "https://gitlab.com" }, PROVIDERS)).toBe("GitLab");
    expect(
      signInName({ kind: "gitlabSelfHosted", baseUrl: "https://gitlab.acme.dev" }, PROVIDERS),
    ).toBe("GitLab acme");
  });

  it("asks to sign in only for a host nobody is signed in to", () => {
    const ctx = makeContext({
      perRemote: [
        makeRemote({
          account: null,
          source: "none",
          url: "https://gitlab.com/a/b.git",
          host: "gitlab.com",
        }),
      ],
    });
    expect(missingSignIn(ctx, ACCOUNTS, PROVIDERS)).toEqual({
      kind: "gitlab",
      baseUrl: "https://gitlab.com",
    });
    // Two GitHub accounts and nothing chosen: a binding is missing, not a sign-in.
    const github = makeContext({
      perRemote: [makeRemote({ account: null, source: "none", host: "github.com" })],
    });
    expect(missingSignIn(github, ACCOUNTS, PROVIDERS)).toBeNull();
    expect(missingSignIn(makeContext(), ACCOUNTS, PROVIDERS)).toBeNull();
  });
});

describe("accounts of another server", () => {
  const github = makeRemote({ remote: "origin", host: "github.com", transport: "https" });

  it("only lets HTTPS remotes use accounts of their own server", () => {
    expect(accountFitsRemote(PERSONAL, github, PROVIDERS)).toBe(true);
    expect(accountFitsRemote(WORK, github, PROVIDERS)).toBe(false);
    // SSH host aliases (`github-work`) cannot be judged by name.
    expect(accountFitsRemote(WORK, { ...github, transport: "ssh" }, PROVIDERS)).toBe(true);
    expect(accountFitsRemote(WORK, github, [])).toBe(true);
  });

  it("reads the refused host from the credential helper error", () => {
    expect(
      refusedHostOf(
        "nanomgg-credential: host `github.com` does not belong to this account (expected `gitlab.com`)",
      ),
    ).toBe("github.com");
    expect(refusedHostOf("fatal: Authentication failed")).toBeNull();
  });

  it("finds the remote meant for the bound account and the accounts of the refused server", () => {
    const ctx = makeContext({
      perRemote: [{ ...github, account: WORK, inherited: false }, makeRemote({ remote: "gitlab" })],
    });
    const fix = hostMismatchFix(ctx, ACCOUNTS, PROVIDERS, "github.com")!;
    expect(fix.remote.remote).toBe("origin");
    expect(fix.accountHost).toBe("gitlab.acme.dev");
    expect(fix.target?.remote).toBe("gitlab");
    expect(fix.candidates.map((a) => a.id)).toEqual([PERSONAL.id, OSS.id]);
    expect(fix.signIn).toBeNull();
    const alone = hostMismatchFix(ctx, [WORK], PROVIDERS, "github.com")!;
    expect(alone.signIn).toEqual({ kind: "github", baseUrl: "https://github.com" });
    expect(hostMismatchFix(ctx, ACCOUNTS, PROVIDERS, "bitbucket.org")).toBeNull();
  });
});
