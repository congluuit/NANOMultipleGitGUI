import { describe, expect, it } from "vitest";
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
  initialOf,
  providerNameOf,
  ruleSummary,
  sortAccounts,
  toAccountSummary,
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
