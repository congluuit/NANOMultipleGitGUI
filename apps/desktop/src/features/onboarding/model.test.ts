import { describe, expect, it } from "vitest";
import {
  buildSelection,
  fillMissingAccounts,
  guessAccount,
  initialState,
  isEmptyScan,
  keyCandidates,
  needsAccount,
  pendingSignIns,
  providerHost,
  selectionSize,
  setIdentityAccount,
} from "./model";
import { PROVIDERS, account, makeScan } from "./testData";

const WORK = account("acc_work", "gl-acme", "an.nguyen");
const PERSONAL = account("acc_me", "github", "an-dev");
const OTHER_GH = account("acc_oss", "github", "an-oss");

describe("keyCandidates", () => {
  it("merges hosts per key file, skips wildcards and flags known/missing keys", () => {
    const keys = keyCandidates(makeScan());
    expect(keys.map((k) => k.path)).toEqual([
      "C:/Users/an/.ssh/id_acme",
      "C:/Users/an/.ssh/id_personal",
      "C:/Users/an/.ssh/id_gone",
    ]);
    expect(keys[0]!.aliases.map((a) => a.alias)).toEqual(["gitlab-acme", "gitlab-acme-2"]);
    expect(keys[1]!.known).toBe(true);
    expect(keys[2]!.exists).toBe(false);
  });
});

describe("guessAccount", () => {
  const all = [WORK, PERSONAL, OTHER_GH];
  it("uses the only account on the host, ignoring the port", () => {
    expect(providerHost(PROVIDERS[1]!)).toBe("gitlab.acme.vn");
    expect(guessAccount(all, PROVIDERS, "gitlab.acme.vn")).toBe("acc_work");
  });
  it("is ambiguous with two accounts on a host unless the user name matches", () => {
    expect(guessAccount(all, PROVIDERS, "github.com")).toBeNull();
    expect(guessAccount(all, PROVIDERS, "github.com", "AN-DEV")).toBe("acc_me");
  });
  it("falls back to the single account", () => {
    expect(guessAccount([PERSONAL], PROVIDERS, null)).toBe("acc_me");
    expect(guessAccount([PERSONAL], PROVIDERS, "unknown.host")).toBe("acc_me");
    expect(guessAccount([], PROVIDERS, "github.com")).toBeNull();
    expect(guessAccount(all, PROVIDERS, null)).toBeNull();
  });
});

describe("initialState", () => {
  it("ticks importable items and guesses accounts from hosts and key paths", () => {
    const scan = makeScan();
    const s = initialState(scan, [WORK, PERSONAL], PROVIDERS);
    // Identity without email cannot be imported; the ACME one follows its ssh key's host.
    expect(s.identities.map((c) => c.checked)).toEqual([true, true, false]);
    expect(s.identities[1]!.accountId).toBe("acc_work");
    expect(s.identities[0]!.accountId).toBeNull();
    // The work rule inherits the identity's account; the remote rule guesses by host.
    expect(s.rules.map((c) => c.accountId)).toEqual(["acc_work", "acc_me"]);
    // Known and missing keys start unticked.
    expect(s.keys.map((c) => c.checked)).toEqual([true, false, false]);
    expect(s.keys[0]!.accountId).toBe("acc_work");
  });

  it("without accounts nothing has an account and identities/rules need one", () => {
    const s = initialState(makeScan(), [], PROVIDERS);
    expect(s.identities.every((c) => c.accountId === null)).toBe(true);
    expect(needsAccount("identity", s.identities[0]!)).toBe(true);
    expect(needsAccount("key", s.keys[0]!)).toBe(false);
    const sel = buildSelection(makeScan(), s);
    expect(sel.identities).toEqual([]);
    expect(sel.rules).toEqual([]);
    expect(sel.sshKeys).toEqual([
      { path: "C:/Users/an/.ssh/id_acme", accountId: null, passphrase: null },
    ]);
  });
});

describe("account changes", () => {
  it("rules follow their identity's account", () => {
    const scan = makeScan();
    let s = initialState(scan, [WORK, PERSONAL], PROVIDERS);
    s = setIdentityAccount(s, scan, 1, "acc_me");
    expect(s.identities[1]!.accountId).toBe("acc_me");
    expect(s.rules[0]!.accountId).toBe("acc_me");
    // A rule the user moved elsewhere is left alone.
    s = { ...s, rules: s.rules.map((r, i) => (i === 0 ? { ...r, accountId: "acc_work" } : r)) };
    s = setIdentityAccount(s, scan, 1, null);
    expect(s.rules[0]!.accountId).toBe("acc_work");
  });

  it("fills only missing accounts after a sign-in", () => {
    const scan = makeScan();
    let s = initialState(scan, [], PROVIDERS);
    s = { ...s, keys: s.keys.map((k, i) => (i === 1 ? { ...k, accountId: "acc_x" } : k)) };
    const filled = fillMissingAccounts(s, scan, [WORK], PROVIDERS);
    expect(filled.identities[0]!.accountId).toBe("acc_work");
    expect(filled.keys[1]!.accountId).toBe("acc_x");
  });
});

describe("buildSelection", () => {
  it("sends ticked items with accounts, identity links and passphrases", () => {
    const scan = makeScan();
    let s = initialState(scan, [WORK, PERSONAL], PROVIDERS);
    s = setIdentityAccount(s, scan, 0, "acc_me");
    s = {
      ...s,
      identities: s.identities.map((c, i) => (i === 1 ? { ...c, makeDefault: true } : c)),
      keys: s.keys.map((k, i) => (i === 0 ? { ...k, passphrase: "pw" } : k)),
    };
    const sel = buildSelection(scan, s, [WORK, PERSONAL]);
    expect(sel.identities).toEqual([
      { accountId: "acc_me", name: "An Nguyen", email: "an@personal.dev", makeDefault: false },
      { accountId: "acc_work", name: "An (ACME)", email: "an@acme.vn", makeDefault: true },
    ]);
    expect(sel.rules).toEqual([
      {
        accountId: "acc_work",
        pathGlob: "C:/Users/an/work/**",
        host: null,
        ownerOrGroup: null,
        remoteRegex: null,
        identityEmail: "an@acme.vn",
      },
      {
        accountId: "acc_me",
        pathGlob: null,
        host: "github.com",
        ownerOrGroup: "oss",
        remoteRegex: null,
        identityEmail: "an@personal.dev",
      },
    ]);
    expect(sel.sshKeys).toEqual([
      { path: "C:/Users/an/.ssh/id_acme", accountId: "acc_work", passphrase: "pw" },
    ]);
    // gh account an-dev already added → nothing left to sign in.
    expect(sel.accounts).toEqual([]);
    expect(sel.markDone).toBe(true);
    expect(selectionSize(sel)).toBe(5);
  });

  it("drops the identity link when the identity goes to another account or is unticked", () => {
    const scan = makeScan();
    let s = initialState(scan, [WORK, PERSONAL], PROVIDERS);
    s = {
      ...s,
      identities: s.identities.map((c, i) => (i === 1 ? { ...c, checked: false } : c)),
    };
    const sel = buildSelection(scan, s);
    expect(sel.rules?.[0]?.identityEmail).toBeNull();
  });

  it("lists gh/glab accounts that still need a sign-in", () => {
    expect(pendingSignIns(makeScan(), [])).toEqual([{ host: "github.com", username: "an-dev" }]);
    expect(pendingSignIns(makeScan(), [PERSONAL])).toEqual([]);
  });

  it("detects an empty scan", () => {
    const scan = makeScan();
    expect(isEmptyScan(scan)).toBe(false);
    scan.report = { ...scan.report, identities: [], rules: [], sshHosts: [], accounts: [] };
    expect(isEmptyScan(scan)).toBe(true);
  });
});
