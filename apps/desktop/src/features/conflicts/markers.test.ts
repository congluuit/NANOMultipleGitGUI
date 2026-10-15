import { describe, expect, it } from "vitest";
import {
  blockAt,
  blockEdit,
  locate,
  parseConflicts,
  replacement,
  resolveAll,
  resolveBlock,
  splitLines,
  stepBlock,
} from "./markers";

const MERGE = [
  "top",
  "<<<<<<< HEAD",
  "ours 1",
  "ours 2",
  "=======",
  "theirs 1",
  ">>>>>>> feature",
  "middle",
  "<<<<<<< HEAD",
  "=======",
  "only theirs",
  ">>>>>>> feature",
  "end",
  "",
].join("\n");

const DIFF3 = [
  "<<<<<<< HEAD",
  "o",
  "||||||| merged common ancestors",
  "b",
  "=======",
  "t",
  ">>>>>>> 1234abc (pick)",
  "",
].join("\n");

describe("parseConflicts", () => {
  it("parses blocks with labels and line ranges", () => {
    const blocks = parseConflicts(MERGE);
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toMatchObject({
      start: 1,
      end: 6,
      mid: 4,
      baseAt: null,
      ours: ["ours 1", "ours 2"],
      base: null,
      theirs: ["theirs 1"],
      oursLabel: "HEAD",
      theirsLabel: "feature",
    });
    expect(blocks[1]).toMatchObject({ start: 8, end: 11, ours: [], theirs: ["only theirs"] });
  });

  it("reads the base section of diff3 / zdiff3 markers", () => {
    const [b] = parseConflicts(DIFF3);
    expect(b).toMatchObject({
      ours: ["o"],
      base: ["b"],
      theirs: ["t"],
      baseLabel: "merged common ancestors",
      theirsLabel: "1234abc (pick)",
    });
  });

  it("handles CRLF files and ignores look-alikes and unfinished blocks", () => {
    const crlf = MERGE.replace(/\n/g, "\r\n");
    expect(splitLines(crlf).eol).toBe("\r\n");
    expect(parseConflicts(crlf)[0]!.ours).toEqual(["ours 1", "ours 2"]);
    expect(parseConflicts("<<<<<<<< x\n========\n>>>>>>>> x\n")).toHaveLength(0);
    expect(parseConflicts("<<<<<<< HEAD\na\n=======\nb\n")).toHaveLength(0);
    expect(parseConflicts("no conflicts\n")).toHaveLength(0);
  });
});

describe("resolveBlock", () => {
  it("applies each choice to one block only", () => {
    expect(resolveBlock(MERGE, 0, "ours").split("\n").slice(0, 4)).toEqual([
      "top",
      "ours 1",
      "ours 2",
      "middle",
    ]);
    expect(resolveBlock(MERGE, 0, "theirs").split("\n").slice(0, 3)).toEqual([
      "top",
      "theirs 1",
      "middle",
    ]);
    const both = resolveBlock(MERGE, 0, "oursThenTheirs");
    expect(both.split("\n").slice(1, 4)).toEqual(["ours 1", "ours 2", "theirs 1"]);
    expect(parseConflicts(both)).toHaveLength(1);
    const rev = resolveBlock(MERGE, 0, "theirsThenOurs");
    expect(rev.split("\n").slice(1, 4)).toEqual(["theirs 1", "ours 1", "ours 2"]);
    expect(resolveBlock(DIFF3, 0, "base")).toBe("b\n");
    // Out of range: unchanged.
    expect(resolveBlock(MERGE, 5, "ours")).toBe(MERGE);
  });

  it("keeps CRLF line endings and resolves everything", () => {
    const crlf = MERGE.replace(/\n/g, "\r\n");
    const out = resolveAll(crlf, "theirs");
    expect(out).toBe(["top", "theirs 1", "middle", "only theirs", "end", ""].join("\r\n"));
    expect(replacement(parseConflicts(MERGE)[1]!, "ours")).toEqual([]);
  });
});

describe("navigation", () => {
  const blocks = parseConflicts(MERGE);
  it("finds the block at or after a line", () => {
    expect(blockAt(blocks, 0)).toBe(0);
    expect(blockAt(blocks, 3)).toBe(0);
    expect(blockAt(blocks, 7)).toBe(1);
    expect(blockAt(blocks, 12)).toBe(-1);
  });
  it("steps forwards and backwards with wrap-around", () => {
    expect(stepBlock(blocks, 0, 1)).toBe(0);
    expect(stepBlock(blocks, 1, 1)).toBe(1);
    expect(stepBlock(blocks, 9, 1)).toBe(0);
    expect(stepBlock(blocks, 9, -1)).toBe(0);
    expect(stepBlock(blocks, 0, -1)).toBe(1);
    expect(stepBlock([], 0, 1)).toBe(-1);
  });
});

describe("blockEdit", () => {
  const len = (lines: string[]) => (n: number) => lines[n - 1]!.length;
  const doc = ["a", "<<<<<<< x", "o", "=======", "t", ">>>>>>> y", "z"];
  it("replaces the block lines", () => {
    expect(blockEdit({ start: 1, end: 5 }, ["o"], doc.length, len(doc))).toEqual({
      startLine: 2,
      startColumn: 1,
      endLine: 6,
      endColumn: 10,
      lines: ["o"],
    });
  });
  it("removes the lines with their line break when the replacement is empty", () => {
    expect(blockEdit({ start: 1, end: 5 }, [], doc.length, len(doc))).toMatchObject({
      startLine: 2,
      startColumn: 1,
      endLine: 7,
      endColumn: 1,
    });
    const tail = ["a", "<<<<<<< x", "=======", ">>>>>>> y"];
    expect(blockEdit({ start: 1, end: 3 }, [], tail.length, len(tail))).toMatchObject({
      startLine: 1,
      startColumn: 2,
      endLine: 4,
      endColumn: 10,
    });
    const only = ["<<<<<<< x", "=======", ">>>>>>> y"];
    expect(blockEdit({ start: 0, end: 2 }, [], only.length, len(only))).toMatchObject({
      startLine: 1,
      endLine: 3,
      lines: [],
    });
  });
});

describe("locate", () => {
  it("finds a run of lines", () => {
    expect(locate(["a", "b", "c", "b", "d"], ["b", "d"])).toBe(3);
    expect(locate(["a"], ["x"])).toBe(-1);
    expect(locate(["a"], [])).toBe(-1);
  });
});
