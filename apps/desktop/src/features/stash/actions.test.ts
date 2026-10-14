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
import { hasLocalChanges, topStash } from "./useStashToolbar";

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
        { index: 0, message: "On main: x" },
        { index: 1, message: "On main: y" },
      ]),
    ).toEqual({ index: 0, message: "On main: x" });
  });
});
