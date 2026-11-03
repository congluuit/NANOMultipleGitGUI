import { describe, expect, it } from "vitest";
import { matchLength } from "./search";

const plain = { caseSensitive: false, wholeWord: false, regex: false, untracked: false };

describe("matchLength", () => {
  it("uses the query length for plain text", () => {
    expect(matchLength("say Hello there", 4, "hello", plain)).toBe(5);
    // Never past the end of the preview.
    expect(matchLength("abc", 2, "cdef", plain)).toBe(1);
  });

  it("matches a regex at the reported column", () => {
    const re = { ...plain, regex: true };
    expect(matchLength("let x = foo123;", 8, "foo[0-9]+", re)).toBe(6);
    expect(matchLength("FOO1", 0, "foo[0-9]", re)).toBe(4);
    // No match: the query length, cut at the end of the line.
    expect(matchLength("FOO1", 0, "foo[0-9]", { ...re, caseSensitive: true })).toBe(4);
    expect(matchLength("abc", 0, "(", re)).toBe(1);
  });
});
