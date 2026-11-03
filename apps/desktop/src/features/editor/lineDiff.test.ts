import { describe, expect, it } from "vitest";
import {
  applyChange,
  changeAt,
  changeKind,
  diffLines,
  originalLine,
  revertEdit,
  revertLines,
  splitLines,
  type LineEdit,
} from "./lineDiff";

const lines = (s: string) => s.split("");

/** Applies a 1-based Monaco-style edit to text (lines joined by `eol`). */
function apply(text: string, e: LineEdit, eol: string): string {
  const ls = text.split(eol);
  const offset = (line: number, col: number) =>
    ls.slice(0, line - 1).reduce((n, l) => n + l.length + eol.length, 0) + col - 1;
  return (
    text.slice(0, offset(e.startLine, e.startColumn)) +
    e.text +
    text.slice(offset(e.endLine, e.endColumn))
  );
}

describe("diffLines", () => {
  it("finds nothing in equal input", () => {
    expect(diffLines(lines("abc"), lines("abc"))).toEqual([]);
  });

  it("reports additions, deletions and modifications", () => {
    const changes = diffLines(lines("abcdef"), lines("abXdfgh"));
    // a b d f stay: c becomes X, e is deleted, g h are added.
    expect(changes).toEqual([
      { originalStart: 2, originalCount: 1, modifiedStart: 2, modifiedCount: 1 },
      { originalStart: 4, originalCount: 1, modifiedStart: 4, modifiedCount: 0 },
      { originalStart: 6, originalCount: 0, modifiedStart: 5, modifiedCount: 2 },
    ]);
    expect(changes.map(changeKind)).toEqual(["modified", "deleted", "added"]);
    expect(diffLines(lines("ac"), lines("abc")).map(changeKind)).toEqual(["added"]);
    expect(diffLines(lines("abc"), lines("ac")).map(changeKind)).toEqual(["deleted"]);
  });

  it("keeps common lines matched in the middle", () => {
    expect(diffLines(lines("xaby"), lines("zabw"))).toEqual([
      { originalStart: 0, originalCount: 1, modifiedStart: 0, modifiedCount: 1 },
      { originalStart: 3, originalCount: 1, modifiedStart: 3, modifiedCount: 1 },
    ]);
  });

  it("falls back to one block past the cost limit", () => {
    const changes = diffLines(lines("abcdefgh"), lines("12345678"), 3);
    expect(changes).toEqual([
      { originalStart: 0, originalCount: 8, modifiedStart: 0, modifiedCount: 8 },
    ]);
  });

  it("reverting every change restores the original", () => {
    const cases: [string, string][] = [
      ["abcdef", "abXdfgh"],
      ["", "abc"],
      ["abc", ""],
      ["axbxcx", "xxx"],
      ["kitten", "sitting"],
    ];
    for (const [a, b] of cases) {
      const orig = lines(a);
      let cur = lines(b);
      // Revert from the bottom so earlier line numbers stay valid.
      for (const c of diffLines(orig, cur).reverse()) cur = revertLines(orig, cur, c);
      expect(cur.join("")).toBe(a);
    }
  });
});

describe("revertEdit", () => {
  const cases: [string, string][] = [
    ["a\nb\nc\n", "a\nX\nc\n"], // middle
    ["a\n", "a\nb\n"], // added before the final empty line
    ["a", "a\nb"], // added at the very end, no final newline
    ["a\nx", "a\ny"], // last line changed
    ["a\nb\n", "a\nb"], // final newline removed
    ["a\nb\nc", "a\nc"], // middle deletion
    ["", "x"], // whole file
    ["x\ny", ""], // everything deleted
    ["a\nb\nc\nd", "Z\nb\nd\nE"], // several blocks
  ];
  for (const eol of ["\n", "\r\n"]) {
    it(`restores the original with ${JSON.stringify(eol)}`, () => {
      for (const [a0, b0] of cases) {
        const a = a0.replaceAll("\n", eol);
        let text = b0.replaceAll("\n", eol);
        const orig = splitLines(a);
        for (const c of diffLines(orig, splitLines(text)).reverse()) {
          text = apply(text, revertEdit(orig, splitLines(text), c, eol), eol);
        }
        expect(text).toBe(a);
      }
    });
  }
});

describe("applyChange / originalLine", () => {
  it("applying every change turns the original into the modified text", () => {
    const cases: [string, string][] = [
      ["abcdef", "abXdfgh"],
      ["", "abc"],
      ["abc", ""],
      ["kitten", "sitting"],
    ];
    for (const [a, b] of cases) {
      const mod = lines(b);
      let cur = lines(a);
      for (const c of diffLines(cur, mod).reverse()) cur = applyChange(cur, mod, c);
      expect(cur.join("")).toBe(b);
    }
  });

  it("stages one change and leaves the others", () => {
    const orig = lines("abcdef");
    const mod = lines("aXcdeY");
    const [first] = diffLines(orig, mod);
    expect(applyChange(orig, mod, first!).join("")).toBe("aXcdef");
  });

  it("maps unchanged lines back to the original, changed ones to null", () => {
    // a b c d e  becomes  a X c N d e: b edited, N inserted.
    const changes = diffLines(lines("abcde"), lines("aXcNde"));
    expect([0, 1, 2, 3, 4, 5].map((l) => originalLine(changes, l))).toEqual([
      0,
      null,
      2,
      null,
      3,
      4,
    ]);
    // Deleted lines shift the rest up.
    expect(originalLine(diffLines(lines("abc"), lines("ac")), 1)).toBe(2);
  });
});

describe("helpers", () => {
  it("splitLines handles every line break and a BOM", () => {
    expect(splitLines("\uFEFFa\r\nb\rc\nd")).toEqual(["a", "b", "c", "d"]);
    expect(splitLines("a\n")).toEqual(["a", ""]);
  });

  it("changeAt maps a line to its block, deletions to the line above", () => {
    const changes = diffLines(lines("abcdef"), lines("aXcef"));
    // b becomes X at line 1, d is deleted after line 2 (c).
    expect(changeAt(changes, 1)).toBe(0);
    expect(changeAt(changes, 2)).toBe(1);
    expect(changeAt(changes, 0)).toBe(-1);
  });
});
