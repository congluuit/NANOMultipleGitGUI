import { describe, expect, it } from "vitest";
import type { RepoRefs } from "@/ipc";
import { headState, matchesFilter, primaryRemote, transportOf } from "./refsView";

describe("transportOf", () => {
  it.each([
    ["https://github.com/acme/api.git", "HTTPS"],
    ["http://127.0.0.1:8080/a.git", "HTTP"],
    ["git@github.com:acme/api.git", "SSH"],
    ["ssh://git@gitlab.acme.dev:2222/acme/api.git", "SSH"],
    ["file:///srv/repo.git", "LOCAL"],
    ["C:\\repos\\api.git", "LOCAL"],
    ["/srv/git/api.git", "LOCAL"],
    [null, null],
  ])("%s → %s", (url, expected) => {
    expect(transportOf(url)).toBe(expected);
  });
});

describe("matchesFilter", () => {
  it("is case-insensitive and empty matches all", () => {
    expect(matchesFilter("feature/Login", "login")).toBe(true);
    expect(matchesFilter("main", "  ")).toBe(true);
    expect(matchesFilter("main", "dev")).toBe(false);
  });
});

const refs = (over: Partial<RepoRefs>): RepoRefs => ({
  head: { branch: "main", oid: "abc1234567", detached: false, unborn: false },
  local: [],
  remotes: [],
  tags: [],
  stashes: [],
  ...over,
});

describe("primaryRemote", () => {
  it("prefers the current branch's upstream remote, then origin", () => {
    const remotes = [
      { name: "origin", url: "https://a", branches: [] },
      { name: "upstream", url: "git@b:x", branches: [] },
    ];
    const local = [
      {
        name: "main",
        oid: "1",
        upstream: "upstream/main",
        ahead: 0,
        behind: 0,
        upstreamGone: false,
        isHead: true,
      },
    ];
    expect(primaryRemote(refs({ remotes, local }))?.name).toBe("upstream");
    expect(primaryRemote(refs({ remotes }))?.name).toBe("origin");
    expect(primaryRemote(refs({}))).toBeNull();
  });
});

describe("headState", () => {
  it("distinguishes branch, unborn and detached HEAD", () => {
    expect(headState(refs({}).head)).toEqual({ kind: "branch", branch: "main" });
    expect(headState({ branch: "main", oid: null, detached: false, unborn: true })).toEqual({
      kind: "unborn",
      branch: "main",
    });
    expect(headState({ branch: null, oid: "abcdef123", detached: true, unborn: false })).toEqual({
      kind: "detached",
      oid: "abcdef1",
    });
  });
});
