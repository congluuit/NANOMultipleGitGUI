import { describe, expect, it } from "vitest";
import type { BranchSwitchCheck } from "@/ipc";
import { DEFAULT_PREFS, folderOf, planSwitch, targetLabel } from "./model";

const clean: BranchSwitchCheck = {
  branch: "feature",
  create: false,
  current: "main",
  alreadyCurrent: false,
  operation: null,
  conflicts: 0,
  changes: 0,
  checkedOutAt: null,
  detachedOrphan: false,
  headShort: null,
  unborn: false,
};

const idle = { transferRunning: false };

describe("planSwitch", () => {
  it("switches right away when the working tree is clean", () => {
    expect(planSwitch(clean, DEFAULT_PREFS, idle)).toEqual({ kind: "go", mode: "stash" });
    expect(planSwitch(clean, { ...DEFAULT_PREFS, dirtyAction: "carry" }, idle).kind).toBe("go");
  });

  it("does nothing on the current branch", () => {
    expect(planSwitch({ ...clean, alreadyCurrent: true, changes: 3 }, DEFAULT_PREFS, idle)).toEqual(
      { kind: "same" },
    );
  });

  it("blocks on an unfinished operation or conflicts before anything else", () => {
    const merging = { ...clean, operation: "merge" as const, changes: 2 };
    expect(planSwitch(merging, DEFAULT_PREFS, { transferRunning: true }).kind).toBe("operation");
    expect(planSwitch({ ...clean, conflicts: 1 }, DEFAULT_PREFS, idle).kind).toBe("operation");
  });

  it("waits for a running transfer", () => {
    expect(planSwitch(clean, DEFAULT_PREFS, { transferRunning: true }).kind).toBe("transfer");
  });

  it("points to the other worktree", () => {
    expect(planSwitch({ ...clean, checkedOutAt: "C:/src/wt" }, DEFAULT_PREFS, idle)).toEqual({
      kind: "worktree",
      path: "C:/src/wt",
    });
  });

  it("warns about orphan detached commits unless confirmed", () => {
    const detached = { ...clean, current: null, detachedOrphan: true, changes: 1 };
    expect(planSwitch(detached, DEFAULT_PREFS, idle).kind).toBe("detached");
    expect(planSwitch(detached, DEFAULT_PREFS, { ...idle, detachedConfirmed: true }).kind).toBe(
      "dirty",
    );
  });

  it("asks about uncommitted changes, or uses the remembered answer", () => {
    const dirty = { ...clean, changes: 4 };
    expect(planSwitch(dirty, DEFAULT_PREFS, idle)).toEqual({ kind: "dirty" });
    expect(planSwitch(dirty, { ...DEFAULT_PREFS, dirtyAction: "stash" }, idle)).toEqual({
      kind: "go",
      mode: "stash",
    });
    expect(planSwitch(dirty, { ...DEFAULT_PREFS, dirtyAction: "carry" }, idle)).toEqual({
      kind: "go",
      mode: "carry",
    });
  });
});

describe("labels", () => {
  it("names targets and folders", () => {
    expect(targetLabel({ kind: "local", name: "main" })).toBe("main");
    expect(targetLabel({ kind: "remote", remote: "origin", branch: "feat/x" })).toBe(
      "origin/feat/x",
    );
    expect(folderOf("C:/src/app-wt/")).toBe("app-wt");
    expect(folderOf("D:\\work\\repo")).toBe("repo");
  });
});
