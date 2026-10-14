/** Pure decision logic of Commit Guard / Push Guard (PLAN §4.6, §6.3). */
import type {
  AccountSuggestion,
  CommitFinding,
  CommitGuardReport,
  FixAuthorMode,
  GuardHeavyBlob,
  PushGuardReport,
} from "@/ipc";

export type CommitGuardDecision = "proceed" | "cancel" | "switched";
export type PushGuardDecision = "proceed" | "cancel";

/** The commit dialog is shown only when there is something to say. */
export function commitNeedsReview(report: CommitGuardReport): boolean {
  return report.findings.length > 0;
}

/** Distinct "switch to account X" candidates over all findings, best first (at most `max`). */
export function switchSuggestions(report: CommitGuardReport, max = 3): AccountSuggestion[] {
  const out: AccountSuggestion[] = [];
  for (const f of report.findings) {
    if (f.kind !== "emailMismatch" && f.kind !== "unbound") continue;
    for (const s of f.suggestions) {
      if (!out.some((o) => o.accountId === s.accountId)) out.push(s);
    }
  }
  return out.slice(0, max);
}

/** Findings in display order: mismatch / unbound first, then notes. */
export function orderedFindings(report: CommitGuardReport): CommitFinding[] {
  const rank: Record<CommitFinding["kind"], number> = {
    emailMismatch: 0,
    unbound: 1,
    emailNotVerified: 2,
    signingMissing: 3,
  };
  return [...report.findings].sort((a, b) => rank[a.kind] - rank[b.kind]);
}

/** The push dialog is shown when there are findings (mismatch, permission, heavy blobs). */
export function pushNeedsReview(report: PushGuardReport): boolean {
  return report.findings.length > 0;
}

/** "Vẫn push" is impossible when the push would certainly fail. */
export function canPushAnyway(report: PushGuardReport): boolean {
  return !report.blocked;
}

export type FixAvailability = { available: true } | { available: false; reason: FixBlocker };
export type FixBlocker = "notCurrent" | "shared" | "lastFine" | "nothing";

/** Whether a "Sửa tác giả" mode can run, and why not. */
export function fixAvailability(report: PushGuardReport, mode: FixAuthorMode): FixAvailability {
  if (report.authorMismatch.length === 0 || report.totalUnpushed === 0) {
    return { available: false, reason: "nothing" };
  }
  if (!report.isCurrent) return { available: false, reason: "notCurrent" };
  if (mode === "amendLast") {
    const head = report.unpushed[0];
    if (!head || head.shared) return { available: false, reason: "lastFine" };
    const mismatched = report.authorMismatch.some((c) => c.oid === head.oid);
    return mismatched ? { available: true } : { available: false, reason: "lastFine" };
  }
  return report.rewritable ? { available: true } : { available: false, reason: "shared" };
}

/** Amend is enough when the only mismatched commit is the last one. */
export function preferredFixMode(report: PushGuardReport): FixAuthorMode {
  const head = report.unpushed[0];
  const only = report.authorMismatch.length === 1 ? report.authorMismatch[0] : undefined;
  return head && only?.oid === head.oid ? "amendLast" : "rewriteUnpushed";
}

/** Heavy blobs that "Chuyển sang LFS" may rewrite (introduced by unpushed commits only). */
export function migratableBlobs(report: PushGuardReport): GuardHeavyBlob[] {
  return report.heavyBlobs.filter((b) => !b.inSharedCommit);
}

/** Heavy blobs in already-pushed history: explained, never rewritten. */
export function sharedBlobs(report: PushGuardReport): GuardHeavyBlob[] {
  return report.heavyBlobs.filter((b) => b.inSharedCommit);
}

/** Distinct paths to pass to `guard_lfs_migrate`. */
export function migratePaths(report: PushGuardReport): string[] {
  return [...new Set(migratableBlobs(report).map((b) => b.path))];
}

/** Whether the LFS migration can run now. */
export function canMigrate(report: PushGuardReport): boolean {
  return report.isCurrent && report.rewritable && migratePaths(report).length > 0;
}

/** Permission denial finding, if any. */
export function permissionDenied(report: PushGuardReport) {
  return report.findings.find((f) => f.kind === "permissionDenied") ?? null;
}
