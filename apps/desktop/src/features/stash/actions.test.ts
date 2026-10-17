import { beforeAll, describe, expect, it } from "vitest";
import { initI18n } from "@/i18n";
import type { StatusEntry, WorkingTreeStatus } from "@/ipc";
import {
  applyOutcomeText,
  buildStashOptions,
  DEFAULT_STASH_FORM,
  shortList,
  stashName,
} from "./actions";
import { defaultStash, hasLocalChanges, popMode, stashTitle, topStash } from "./useStashToolbar";

beforeAll(() => {
  initI18n("vi");
});

const entry = (path: string): StatusEntry => ({
  path,
  oldPath: null,
  kind: "modified",
  conflict: null,
  submodule: false,
  size: 1,
});

const status = (over: Partial<WorkingTreeStatus> = {}): WorkingTreeStatus => ({
  branch: "main",
  headOid: "a".repeat(40),
  unborn: false,
  upstream: null,
  ahead: 0,
  behind: 0,
  staged: [],
  unstaged: [],
  conflicted: [],
  ...over,
});

describe("buildStashOptions", () => {
  it("includes untracked files by default and drops an empty message", () => {
    expect(buildStashOptions(DEFAULT_STASH_FORM)).toEqual({
      message: null,
      includeUntracked: true,
      keepIndex: false,
    });
  });

  it("trims the message and passes the flags", () => {
    expect(
      buildStashOptions({ message: "  wip login  ", includeUntracked: false, keepIndex: true }),
    ).toEqual({ message: "wip login", includeUntracked: false, keepIndex: true });
  });
});

describe("stash helpers", () => {
  it("names entries like git", () => {
    expect(stashName(0)).toBe("stash@{0}");
    expect(stashName(12)).toBe("stash@{12}");
  });

  it("shortens long path lists", () => {
    expect(shortList(["a", "b"])).toBe("a, b");
    expect(shortList(["a", "b", "c", "d"])).toBe("a, b, c, …");
  });

  it("describes apply / pop outcomes", () => {
    expect(applyOutcomeText(0, { conflicts: [], dropped: true }, true)).toBe(
      "Đã áp dụng và xóa stash@{0}",
    );
    expect(applyOutcomeText(2, { conflicts: [], dropped: false }, false)).toBe(
      "Đã áp dụng stash@{2}",
    );
    expect(applyOutcomeText(0, { conflicts: ["a.ts", "b.ts"], dropped: false }, true)).toBe(
      "Áp dụng stash@{0} bị xung đột ở 2 file: a.ts, b.ts. Stash vẫn được giữ lại.",
    );
  });

  it("knows when there is something to stash", () => {
    expect(hasLocalChanges(undefined)).toBe(false);
    expect(hasLocalChanges(status())).toBe(false);
    expect(hasLocalChanges(status({ unstaged: [entry("a")] }))).toBe(true);
    expect(hasLocalChanges(status({ staged: [entry("a")] }))).toBe(true);
  });

  it("pops stash@{0}", () => {
    expect(topStash(undefined)).toBeNull();
    expect(topStash([])).toBeNull();
    expect(
      topStash([
        { index: 0, message: "On main: x", time: 0, branch: "main" },
        { index: 1, message: "On main: y", time: 0, branch: "main" },
      ]),
    ).toEqual({ index: 0, message: "On main: x", time: 0, branch: "main" });
  });
});

describe("Pop button", () => {
  const stash = (index: number, branch: string | null, message = "On x: m") => ({
    index,
    message,
    time: 0,
    branch,
  });

  it("pops a single stash of this branch in one click, otherwise lets the user choose", () => {
    expect(popMode([], "main")).toBe("none");
    expect(popMode(undefined, "main")).toBe("none");
    expect(popMode([stash(0, "main")], "main")).toBe("single");
    expect(popMode([stash(0, null)], "main")).toBe("single");
    expect(popMode([stash(0, "feature")], "main")).toBe("otherBranch");
    expect(popMode([stash(0, "main"), stash(1, "main")], "main")).toBe("many");
  });

  it("shows the message without git's prefix", () => {
    expect(stashTitle(stash(0, "main", "On main: half: done"))).toBe("half: done");
    expect(stashTitle(stash(0, "main", "WIP on main: abc1234 subject"))).toBe(
      "WIP · abc1234 subject",
    );
    expect(stashTitle(stash(0, null, "custom"))).toBe("custom");
  });

  it("selects the newest stash of the current branch in the picker", () => {
    const list = [stash(2, "main"), stash(0, "feature"), stash(1, "main")];
    expect(defaultStash(list, "main")).toBe(1);
    expect(defaultStash(list, "other")).toBe(0);
    expect(defaultStash([], "main")).toBeNull();
  });
});
