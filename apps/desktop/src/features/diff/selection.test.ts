import { describe, expect, it } from "vitest";
import type { DiffLine, Hunk } from "@/ipc";
import {
  EMPTY_SEL,
  countSelected,
  hunkChanges,
  isSelected,
  selectRange,
  toSelections,
  toggleLine,
  wholeHunk,
} from "./selection";

const line = (kind: DiffLine["kind"]): DiffLine => ({
  kind,
  text: "x",
  oldLine: null,
  newLine: null,
  noNewline: false,
});

const hunk = (...kinds: DiffLine["kind"][]): Hunk => ({
  header: "@@ -1 +1 @@",
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  lines: kinds.map(line),
});

const hunks = [hunk("context", "delete", "add", "context", "add"), hunk("add", "add", "context")];

describe("line selection", () => {
  it("lists changed lines of a hunk", () => {
    expect(hunkChanges(hunks[0]!)).toEqual([1, 2, 4]);
    expect(wholeHunk(hunks, 1)).toEqual([{ hunk: 1, lines: [0, 1] }]);
    expect(wholeHunk(hunks, 5)).toEqual([]);
  });

  it("toggles changed lines and ignores context", () => {
    let sel = toggleLine(EMPTY_SEL, hunks, 0, 2);
    sel = toggleLine(sel, hunks, 0, 0);
    sel = toggleLine(sel, hunks, 1, 1);
    expect(isSelected(sel, 0, 2)).toBe(true);
    expect(isSelected(sel, 0, 0)).toBe(false);
    expect(countSelected(sel)).toBe(2);
    sel = toggleLine(sel, hunks, 0, 2);
    expect(toSelections(sel)).toEqual([{ hunk: 1, lines: [1] }]);
  });

  it("selects ranges within a hunk in any direction", () => {
    const sel = selectRange(EMPTY_SEL, hunks, 0, 4, 1);
    expect(toSelections(sel)).toEqual([{ hunk: 0, lines: [1, 2, 4] }]);
    expect(selectRange(sel, hunks, 9, 0, 1)).toBe(sel);
  });
});
