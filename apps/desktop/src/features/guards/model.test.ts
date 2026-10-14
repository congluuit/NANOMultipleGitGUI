import { describe, expect, it } from "vitest";
import {
  canMigrate,
  canPushAnyway,
  commitNeedsReview,
  fixAvailability,
  migratePaths,
  orderedFindings,
  permissionDenied,
  preferredFixMode,
  pushNeedsReview,
  sharedBlobs,
  switchSuggestions,
} from "./model";
import { MB, MISMATCH, WORK_SUGGESTION, blob, commit, commitReport, pushReport } from "./testData";

describe("commit guard decisions", () => {
  it("reviews only when there are findings", () => {
    expect(commitNeedsReview(commitReport())).toBe(false);
    expect(commitNeedsReview(MISMATCH)).toBe(true);
  });

  it("dedupes switch suggestions across findings", () => {
    const report = commitReport({
      findings: [
        ...MISMATCH.findings,
        { kind: "unbound", suggestions: [WORK_SUGGESTION, { ...WORK_SUGGESTION, accountId: "b" }] },
      ],
    });
    expect(switchSuggestions(report).map((s) => s.accountId)).toEqual(["acc-work", "b"]);
    expect(switchSuggestions(report, 1)).toHaveLength(1);
  });

  it("orders mismatch first, notes last", () => {
    const report = commitReport({ findings: [...MISMATCH.findings].reverse() });
    expect(orderedFindings(report).map((f) => f.kind)).toEqual([
      "emailMismatch",
      "emailNotVerified",
      "signingMissing",
    ]);
  });
});

describe("push guard decisions", () => {
  it("reviews findings and blocks only certain failures", () => {
    expect(pushNeedsReview(pushReport())).toBe(true);
    expect(pushNeedsReview(pushReport({ findings: [] }))).toBe(false);
    expect(canPushAnyway(pushReport())).toBe(true);
    expect(canPushAnyway(pushReport({ blocked: true }))).toBe(false);
    const denied = pushReport({
      findings: [{ kind: "permissionDenied", accountLabel: "X", level: "read" }],
    });
    expect(permissionDenied(denied)?.kind).toBe("permissionDenied");
    expect(permissionDenied(pushReport())).toBeNull();
  });

  it("offers amend only when the last commit is the mismatched one", () => {
    const head = pushReport();
    expect(fixAvailability(head, "amendLast")).toEqual({ available: true });
    expect(fixAvailability(head, "rewriteUnpushed")).toEqual({ available: true });
    expect(preferredFixMode(head)).toBe("amendLast");

    const older = pushReport({
      unpushed: [commit("c3", "hoang@acme.dev"), commit("c2", "me@gmail.com")],
    });
    expect(fixAvailability(older, "amendLast")).toEqual({ available: false, reason: "lastFine" });
    expect(preferredFixMode(older)).toBe("rewriteUnpushed");
  });

  it("explains why a fix cannot run", () => {
    expect(fixAvailability(pushReport({ isCurrent: false }), "rewriteUnpushed")).toEqual({
      available: false,
      reason: "notCurrent",
    });
    expect(fixAvailability(pushReport({ rewritable: false }), "rewriteUnpushed")).toEqual({
      available: false,
      reason: "shared",
    });
    expect(fixAvailability(pushReport({ authorMismatch: [] }), "amendLast")).toEqual({
      available: false,
      reason: "nothing",
    });
  });

  it("migrates only blobs from unpushed commits", () => {
    const report = pushReport({
      heavyBlobs: [
        blob("a.psd", 120 * MB),
        blob("a.psd", 60 * MB, { oid: "other" }),
        blob("old.zip", 70 * MB, { inSharedCommit: true }),
      ],
    });
    expect(migratePaths(report)).toEqual(["a.psd"]);
    expect(sharedBlobs(report).map((b) => b.path)).toEqual(["old.zip"]);
    expect(canMigrate(report)).toBe(true);
    expect(canMigrate({ ...report, isCurrent: false })).toBe(false);
    expect(canMigrate({ ...report, heavyBlobs: [blob("x", MB, { inSharedCommit: true })] })).toBe(
      false,
    );
  });
});
