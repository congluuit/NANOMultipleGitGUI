/** Report factories shared by the guard tests. */
import type {
  AccountSuggestion,
  CommitGuardReport,
  GuardHeavyBlob,
  PushGuardReport,
  UnpushedCommit,
} from "@/ipc";

export const MB = 1024 * 1024;

export const WORK_SUGGESTION: AccountSuggestion = {
  accountId: "acc-work",
  label: "Công ty",
  username: "hoang-acme",
  color: "#1c7ed6",
  identityId: "idn-work",
  email: "hoang@acme.dev",
  reason: "matchedRule",
};

export function commitReport(over: Partial<CommitGuardReport> = {}): CommitGuardReport {
  return {
    repoId: 1,
    identity: {
      name: "Hoàng",
      email: "hoangnv@gmail.com",
      source: "account",
      accountId: "acc-personal",
      signing: "none",
    },
    accountId: "acc-personal",
    findings: [],
    emailsChecked: true,
    ...over,
  };
}

export const MISMATCH = commitReport({
  findings: [
    {
      kind: "emailMismatch",
      email: "hoangnv@gmail.com",
      reason: "ruleAccount",
      expectedEmail: "hoang@acme.dev",
      expectedAccountId: "acc-work",
      suggestions: [WORK_SUGGESTION],
    },
    {
      kind: "emailNotVerified",
      email: "hoangnv@gmail.com",
      accountId: "acc-personal",
      accountLabel: "Cá nhân",
    },
    { kind: "signingMissing", gpgSign: true, identitySigning: "none" },
  ],
});

export function commit(oid: string, email: string, shared = false): UnpushedCommit {
  return {
    oid,
    shortOid: oid.slice(0, 7),
    summary: `msg ${oid}`,
    authorName: "A",
    authorEmail: email,
    committerName: "A",
    committerEmail: email,
    time: 0,
    shared,
  };
}

export function blob(
  path: string,
  size: number,
  over: Partial<GuardHeavyBlob> = {},
): GuardHeavyBlob {
  return {
    oid: `${path}-oid`,
    path,
    size,
    commits: ["c1"],
    inSharedCommit: false,
    level: size > 100 * MB ? "block" : "warn",
    ...over,
  };
}

export function pushReport(over: Partial<PushGuardReport> = {}): PushGuardReport {
  const unpushed = over.unpushed ?? [
    commit("c3aaaaaaa", "hoangnv@gmail.com"),
    commit("c2bbbbbbb", "hoang@acme.dev"),
  ];
  return {
    repoId: 1,
    remote: "origin",
    branch: "main",
    upstream: "refs/remotes/origin/main",
    identity: {
      name: "Hoàng",
      email: "hoang@acme.dev",
      source: "account",
      accountId: "acc-work",
      signing: "none",
    },
    unpushed,
    totalUnpushed: unpushed.length,
    rewritable: true,
    authorMismatch: unpushed.filter((c) => c.authorEmail !== "hoang@acme.dev"),
    heavyBlobs: [],
    thresholds: { warnBytes: 50 * MB, blockBytes: 100 * MB },
    permission: {
      status: "allowed",
      level: "write",
      accountId: "acc-work",
      accountLabel: "Công ty",
      reason: null,
    },
    findings: [{ kind: "authorMismatch", count: 1, email: "hoang@acme.dev" }],
    blocked: false,
    isCurrent: true,
    ...over,
  };
}
