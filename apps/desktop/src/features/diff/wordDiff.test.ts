import { describe, expect, it } from "vitest";
import type { DiffLine } from "@/ipc";
import { hunkHighlights, inlineChanges, splitByRanges, tokenize } from "./wordDiff";

const line = (kind: DiffLine["kind"], text: string): DiffLine => ({
  kind,
  text,
  oldLine: null,
  newLine: null,
  noNewline: false,
});

const pieces = (text: string, ranges: readonly (readonly [number, number])[]) =>
  splitByRanges(text, ranges)
    .filter((p) => p.changed)
    .map((p) => p.text);

describe("word diff", () => {
  it("splits words, spaces and punctuation", () => {
    expect(tokenize("const a = foo(1);")).toEqual([
      "const",
      " ",
      "a",
      " ",
      "=",
      " ",
      "foo",
      "(",
      "1",
      ")",
      ";",
    ]);
    expect(tokenize("tên biến_mới")).toEqual(["tên", " ", "biến_mới"]);
  });

  it("highlights only the words that changed", () => {
    const oldText = "  const total = price * count;";
    const newText = "  const total = price * quantity;";
    const change = inlineChanges(oldText, newText)!;
    expect(pieces(oldText, change.old)).toEqual(["count"]);
    expect(pieces(newText, change.new)).toEqual(["quantity"]);
  });

  it("marks an insertion on the new line only", () => {
    const change = inlineChanges("call(a, b)", "call(a, b, c)")!;
    expect(change.old).toEqual([]);
    expect(pieces("call(a, b, c)", change.new)).toEqual([", c"]);
  });

  it("gives up when the lines have little in common", () => {
    expect(inlineChanges("return null;", "throw new Error(message);")).toBeNull();
    expect(inlineChanges("same", "same")).toBeNull();
  });

  it("pairs removed lines with the added lines that replace them", () => {
    const lines = [
      line("context", "function f() {"),
      line("delete", "  let x = 1;"),
      line("delete", "  let y = 2;"),
      line("add", "  let x = 10;"),
      line("add", "  let y = 2;"),
      line("context", "}"),
      line("delete", "  gone();"),
      line("add", "  one();"),
      line("add", "  two();"),
    ];
    const marks = hunkHighlights(lines);
    expect(pieces(lines[1]!.text, marks[1]!)).toEqual(["1"]);
    expect(pieces(lines[3]!.text, marks[3]!)).toEqual(["10"]);
    // Identical pair: nothing to highlight.
    expect(marks[2]).toBeNull();
    expect(marks[4]).toBeNull();
    // One removed, two added: not an edit of one line.
    expect(marks.slice(6)).toEqual([null, null, null]);
  });

  it("splits a line into plain and changed pieces", () => {
    expect(
      splitByRanges("abcdef", [
        [1, 3],
        [5, 6],
      ]),
    ).toEqual([
      { text: "a", changed: false },
      { text: "bc", changed: true },
      { text: "de", changed: false },
      { text: "f", changed: true },
    ]);
    expect(splitByRanges("abc", [])).toEqual([{ text: "abc", changed: false }]);
  });
});
