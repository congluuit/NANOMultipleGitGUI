import { describe, expect, it } from "vitest";
import type { RebaseCommit, RebasePreview } from "@/ipc";
import {
  canStartRebase,
  defaultSquashMessage,
  effectiveMessage,
  groupHead,
  hasEditor,
  initialRows,
  isUnchanged,
  moveRow,
  moveRowTo,
  previewResult,
  setAction,
  setMessage,
  toSteps,
  validate,
} from "./model";

const commit = (n: string, extra: Partial<RebaseCommit> = {}): RebaseCommit => ({
  oid: `${n}`.repeat(40).slice(0, 40),
  shortOid: `${n}`.repeat(7).slice(0, 7),
  subject: `commit ${n}`,
  message: `commit ${n}\n\nbody ${n}`,
  authorName: "A",
  time: 0,
  pushed: false,
  isMerge: false,
  ...extra,
});

const commits = [commit("a"), commit("b"), commit("c")];

describe("interactive rebase model", () => {
  it("reorders rows and clamps at the ends", () => {
    const rows = initialRows(commits);
    expect(moveRow(rows, 0, 1).map((r) => r.shortOid[0])).toEqual(["b", "a", "c"]);
    expect(moveRow(rows, 0, -1)).toBe(rows);
    expect(moveRowTo(rows, 2, 0).map((r) => r.shortOid[0])).toEqual(["c", "a", "b"]);
    expect(moveRowTo(rows, 0, 99).map((r) => r.shortOid[0])).toEqual(["b", "c", "a"]);
    expect(isUnchanged(rows, commits)).toBe(true);
    expect(isUnchanged(moveRow(rows, 0, 1), commits)).toBe(false);
  });

  it("combines squash messages into the group head and skips fixups and drops", () => {
    let rows = initialRows(commits);
    rows = setAction(rows, 1, "fixup");
    rows = setAction(rows, 2, "squash");
    expect(groupHead(rows, 2)).toBe(0);
    expect(defaultSquashMessage(rows, 2)).toBe("commit a\n\nbody a\n\ncommit c\n\nbody c");
    expect(hasEditor(rows, 2)).toBe(true);
    expect(hasEditor(rows, 1)).toBe(false);
    rows = setMessage(rows, 2, "a + c");
    expect(effectiveMessage(rows, 2)).toBe("a + c");
    const { commits: result, dropped } = previewResult(rows);
    expect(dropped).toBe(0);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ subject: "a + c", folded: 2, reworded: true });
    expect(toSteps(rows)).toEqual([
      { oid: commits[0]!.oid, action: "pick", message: null },
      { oid: commits[1]!.oid, action: "fixup", message: null },
      { oid: commits[2]!.oid, action: "squash", message: "a + c" },
    ]);
  });

  it("only the last squash of a group carries the message", () => {
    let rows = setAction(setAction(initialRows(commits), 1, "squash"), 2, "squash");
    expect(hasEditor(rows, 1)).toBe(false);
    expect(hasEditor(rows, 2)).toBe(true);
    expect(toSteps(rows)[1]!.message).toBeNull();
    // Changing the action drops a message typed for squash.
    rows = setMessage(rows, 2, "x");
    expect(setAction(rows, 2, "pick")[2]!.custom).toBeNull();
  });

  it("validates squash-first, empty messages and merges", () => {
    let rows = setAction(initialRows(commits), 0, "drop");
    rows = setAction(rows, 1, "squash");
    expect(validate(rows)).toEqual([{ kind: "squashFirst" }]);
    rows = setMessage(setAction(initialRows(commits), 1, "reword"), 1, "  ");
    expect(validate(rows)).toEqual([{ kind: "emptyMessage", oid: "bbbbbbb" }]);
    expect(validate(initialRows([commit("m", { isMerge: true })]))).toEqual([{ kind: "merges" }]);
    // An edit stop may keep its message unchanged.
    expect(validate(setAction(initialRows(commits), 0, "edit"))).toEqual([]);
  });

  it("previews reword, edit and drop", () => {
    let rows = setAction(initialRows(commits), 0, "reword");
    rows = setMessage(rows, 0, "A!");
    rows = setAction(rows, 1, "edit");
    rows = setAction(rows, 2, "drop");
    const { commits: result, dropped } = previewResult(rows);
    expect(dropped).toBe(1);
    expect(result.map((c) => [c.subject, c.reworded, c.stops])).toEqual([
      ["A!", true, false],
      ["commit b", false, true],
    ]);
    expect(toSteps(rows).map((s) => [s.action, s.message])).toEqual([
      ["reword", "A!"],
      ["edit", null],
      ["drop", null],
    ]);
  });

  it("decides when a plain rebase may start", () => {
    const p: RebasePreview = {
      branch: "topic",
      isCurrent: true,
      head: "h",
      onto: "o",
      commits: [],
      total: 2,
      pushed: 1,
      merges: 0,
      dirty: true,
      upToDate: false,
      fastForward: false,
    };
    expect(canStartRebase(p, false, true)).toBe(false);
    expect(canStartRebase(p, true, false)).toBe(false);
    expect(canStartRebase(p, true, true)).toBe(true);
    expect(canStartRebase({ ...p, upToDate: true }, true, true)).toBe(false);
    expect(canStartRebase({ ...p, branch: null }, true, true)).toBe(false);
  });
});
