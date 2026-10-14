import { describe, expect, it } from "vitest";
import type { LocalBranch, RepoRefs } from "@/ipc";
import { newOpId, progressText, pushPlan, splitUpstream } from "./plan";

const branch = (over: Partial<LocalBranch>): LocalBranch => ({
  name: "main",
  oid: "a".repeat(40),
  upstream: null,
  ahead: 0,
  behind: 0,
  upstreamGone: false,
  isHead: true,
  ...over,
});

const refs = (over: Partial<RepoRefs> = {}): RepoRefs => ({
  head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
  local: [branch({})],
  remotes: [
    { name: "origin", url: "https://github.com/a/b.git", branches: [] },
    { name: "origin/mirror", url: null, branches: [] },
  ],
  tags: [],
  stashes: [],
  ...over,
});

describe("splitUpstream", () => {
  it("prefers the longest matching remote name", () => {
    expect(splitUpstream("origin/mirror/x", ["origin", "origin/mirror"])).toEqual({
      remote: "origin/mirror",
      branch: "x",
    });
    expect(splitUpstream("origin/feature/y", ["origin"])).toEqual({
      remote: "origin",
      branch: "feature/y",
    });
    expect(splitUpstream("other/x", ["origin"])).toBeNull();
  });
});

describe("pushPlan", () => {
  it("pushes to the upstream", () => {
    const r = refs({ local: [branch({ upstream: "origin/main" })] });
    expect(pushPlan(r)).toEqual({
      remote: "origin",
      branch: "main",
      remoteBranch: null,
      setUpstream: false,
      forceWithLease: false,
    });
    const renamed = refs({ local: [branch({ upstream: "origin/trunk" })] });
    expect(pushPlan(renamed, true)?.remoteBranch).toBe("trunk");
    expect(pushPlan(renamed, true)?.forceWithLease).toBe(true);
  });

  it("sets the upstream on origin for a new branch", () => {
    expect(pushPlan(refs())).toMatchObject({ remote: "origin", setUpstream: true });
    const gone = refs({ local: [branch({ upstream: "origin/main", upstreamGone: true })] });
    expect(pushPlan(gone)?.setUpstream).toBe(true);
  });

  it("refuses detached heads and repos without remotes", () => {
    expect(
      pushPlan(refs({ head: { branch: null, oid: "a", detached: true, unborn: false } })),
    ).toBeNull();
    expect(pushPlan(refs({ remotes: [] }))).toBeNull();
  });
});

describe("progressText", () => {
  it("formats percent, counts, bytes and speed", () => {
    expect(
      progressText({
        label: "Receiving objects",
        percent: 45,
        current: 450,
        total: 1000,
        bytes: "1.20 MiB",
        throughput: "2.00 MiB/s",
      }),
    ).toBe("Receiving objects 45% · 1.20 MiB · 2.00 MiB/s");
    expect(
      progressText({
        label: "Counting",
        percent: null,
        current: 3,
        total: 9,
        bytes: null,
        throughput: null,
      }),
    ).toBe("Counting 3/9");
  });

  it("creates distinct operation ids", () => {
    expect(newOpId("push")).not.toBe(newOpId("push"));
  });
});
