import { describe, expect, it } from "vitest";
import {
  ROW_PAGE,
  TABLE_MAX_CHARS,
  cellValues,
  detectDelimiter,
  detectHeader,
  diffTables,
  displayItems,
  formatNoise,
  isTablePath,
  isUniqueColumn,
  isValueLike,
  matchColumns,
  pairByKey,
  pairByPosition,
  parseCsv,
  parseSides,
  type ParsedSides,
  type TableDiffOptions,
} from "./csvTable";

const AUTO: TableDiffOptions = { header: null, key: "auto" };
const BOM = String.fromCharCode(0xfeff);

function sides(oldText: string | null, newText: string | null, path = "data.csv"): ParsedSides {
  const r = parseSides(path, oldText, newText);
  if (!r.ok) throw new Error("too large");
  return r.sides;
}

describe("isTablePath and detectDelimiter", () => {
  it("recognises csv and tsv files", () => {
    expect(isTablePath("a/b.CSV")).toBe(true);
    expect(isTablePath("x.tsv")).toBe(true);
    expect(isTablePath("x.csv.bak")).toBe(false);
    expect(isTablePath("notes.txt")).toBe(false);
  });

  it("sniffs the separator from the first line", () => {
    expect(detectDelimiter("a.tsv", "a,b,c")).toBe("\t");
    expect(detectDelimiter("a.csv", "a,b,c\n1,2,3")).toBe(",");
    expect(detectDelimiter("a.csv", "a;b;c\n1;2;3")).toBe(";");
    expect(detectDelimiter("a.csv", "a\tb\n1\t2")).toBe("\t");
    expect(detectDelimiter("a.csv", '"x;y"|b|c')).toBe("|");
    expect(detectDelimiter("a.csv", `${BOM}\n\nname;age`)).toBe(";");
    expect(detectDelimiter("a.csv", "single")).toBe(",");
  });
});

describe("parseCsv", () => {
  it("splits plain records", () => {
    expect(parseCsv("a,b,c\n1,2,3\n", ",")).toEqual([
      ["a", "b", "c"],
      ["1", "2", "3"],
    ]);
  });

  it("handles quoted fields with commas, doubled quotes and line breaks", () => {
    const text = 'id,note\n1,"hello, world"\n2,"say ""hi"""\n3,"two\nlines"\n4,""\n';
    expect(parseCsv(text, ",")).toEqual([
      ["id", "note"],
      ["1", "hello, world"],
      ["2", 'say "hi"'],
      ["3", "two\nlines"],
      ["4", ""],
    ]);
  });

  it("treats CRLF, LF and lone CR line endings alike", () => {
    const want = [
      ["a", "b"],
      ["1", "2"],
    ];
    expect(parseCsv("a,b\r\n1,2\r\n", ",")).toEqual(want);
    expect(parseCsv("a,b\r1,2", ",")).toEqual(want);
    expect(parseCsv('a,"b"\r\n1,2', ",")).toEqual(want);
    // A CRLF inside quotes stays part of the value.
    expect(parseCsv('a,"x\r\ny"', ",")).toEqual([["a", "x\r\ny"]]);
  });

  it("strips a BOM, keeps empty fields and drops trailing blank lines", () => {
    expect(parseCsv(`${BOM}a,,c\n,,\n\n\n`, ",")).toEqual([
      ["a", "", "c"],
      ["", "", ""],
    ]);
    expect(parseCsv("", ",")).toEqual([]);
    expect(parseCsv('"",x', ",")).toEqual([["", "x"]]);
  });

  it("keeps a quote inside an unquoted field and text after a closing quote", () => {
    expect(parseCsv('5" pipe,"ab"c', ",")).toEqual([['5" pipe', "abc"]]);
  });

  it("does not lose data on an unterminated quote", () => {
    expect(parseCsv('a,"open\nstill', ",")).toEqual([["a", "open\nstill"]]);
  });

  it("parses tab separated values", () => {
    expect(parseCsv("a\tb c\n1\t2", "\t")).toEqual([
      ["a", "b c"],
      ["1", "2"],
    ]);
  });
});

describe("detectHeader", () => {
  it("accepts titles over numbers and dates", () => {
    expect(isValueLike("1,234.5")).toBe(true);
    expect(isValueLike("-3e5")).toBe(true);
    expect(isValueLike("2026-11-23")).toBe(true);
    expect(isValueLike("TRUE")).toBe(true);
    expect(isValueLike("Hanoi")).toBe(false);
    expect(isValueLike("")).toBe(false);
    expect(
      detectHeader([
        ["id", "price", "date"],
        ["1", "9.5", "2026-01-02"],
      ]),
    ).toBe(true);
  });

  it("rejects a first row with numbers, blanks or repeats", () => {
    expect(
      detectHeader([
        ["1", "2"],
        ["3", "4"],
      ]),
    ).toBe(false);
    expect(
      detectHeader([
        ["name", ""],
        ["a", "b"],
      ]),
    ).toBe(false);
    expect(
      detectHeader([
        ["x", "x"],
        ["a", "b"],
      ]),
    ).toBe(false);
    expect(detectHeader([])).toBe(false);
  });

  it("rejects a first row whose values repeat below it", () => {
    expect(
      detectHeader([
        ["apple", "red"],
        ["apple", "green"],
        ["pear", "red"],
      ]),
    ).toBe(false);
  });
});

describe("matchColumns", () => {
  it("matches by title, so moved columns stay paired", () => {
    const cols = matchColumns(["id", "a", "b"], ["id", "b", "a"], 3, 3);
    expect(cols.map((c) => [c.name, c.oldIndex, c.newIndex, c.status])).toEqual([
      ["id", 0, 0, "same"],
      ["b", 2, 1, "same"],
      ["a", 1, 2, "same"],
    ]);
  });

  it("marks added, removed and renamed columns", () => {
    const cols = matchColumns(["id", "name", "old", "qty"], ["id", "title", "qty", "new"], 4, 4);
    expect(cols.map((c) => [c.name, c.status, c.oldName])).toEqual([
      ["id", "same", null],
      ["title", "renamed", "name"],
      ["old", "removed", null],
      ["qty", "same", null],
      ["new", "added", null],
    ]);
  });

  it("pairs repeated titles by occurrence", () => {
    const cols = matchColumns(["x", "x"], ["x", "x", "x"], 2, 3);
    expect(cols.map((c) => c.status)).toEqual(["same", "same", "added"]);
  });

  it("matches by position without a header", () => {
    const cols = matchColumns(null, null, 3, 2);
    expect(cols.map((c) => [c.oldIndex, c.newIndex, c.status])).toEqual([
      [0, 0, "same"],
      [1, 1, "same"],
      [2, null, "removed"],
    ]);
  });
});

describe("row pairing", () => {
  it("pairs by key and keeps removed rows near their old neighbours", () => {
    const old = [["1"], ["2"], ["3"], ["4"]];
    const nw = [["1"], ["3"], ["5"], ["4"]];
    expect(pairByKey(old, nw, 0, 0)).toEqual([
      [0, 0],
      [1, null],
      [2, 1],
      [null, 2],
      [3, 3],
    ]);
  });

  it("pairs repeated keys by occurrence and follows moved rows", () => {
    const old = [["a"], ["b"], ["a"]];
    const nw = [["a"], ["a"], ["b"]];
    expect(pairByKey(old, nw, 0, 0)).toEqual([
      [0, 0],
      [2, 1],
      [1, 2],
    ]);
  });

  it("aligns identical rows by position and pairs what is left in order", () => {
    // Inserted row "x" in the middle: no shift of the rows after it.
    expect(pairByPosition(["a", "b", "c"], ["a", "x", "b", "c"])).toEqual([
      [0, 0],
      [null, 1],
      [1, 2],
      [2, 3],
    ]);
    // Edited row: same position, paired as a change.
    expect(pairByPosition(["a", "b", "c"], ["a", "B", "c"])).toEqual([
      [0, 0],
      [1, 1],
      [2, 2],
    ]);
    expect(pairByPosition(["a", "b"], [])).toEqual([
      [0, null],
      [1, null],
    ]);
  });

  it("finds a longest run of equal rows and covers every row once", () => {
    // Reference LCS length by dynamic programming.
    const lcs = (a: string[], b: string[]) => {
      const w = b.length + 1;
      const dp = new Array<number>((a.length + 1) * w).fill(0);
      const at = (i: number, j: number) => dp[i * w + j] ?? 0;
      for (let i = a.length - 1; i >= 0; i--)
        for (let j = b.length - 1; j >= 0; j--)
          dp[i * w + j] =
            a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
      return at(0, 0);
    };
    let seed = 7;
    const rand = (n: number) => {
      seed = (seed * 48271) % 2147483647;
      return seed % n;
    };
    const word = () => Array.from({ length: rand(12) }, () => "abcd".charAt(rand(4)));
    for (let run = 0; run < 200; run++) {
      const a = word();
      const b = word();
      const pairs = pairByPosition(a, b);
      const olds = pairs.map((p) => p[0]).filter((i) => i !== null);
      const news = pairs.map((p) => p[1]).filter((j) => j !== null);
      expect(olds).toEqual(a.map((_, i) => i));
      expect(news).toEqual(b.map((_, j) => j));
      const equal = pairs.filter(([i, j]) => i !== null && j !== null && a[i] === b[j]).length;
      expect(equal).toBeGreaterThanOrEqual(lcs(a, b));
    }
  });

  it("checks key uniqueness", () => {
    expect(isUniqueColumn([["1"], ["2"]], 0)).toBe(true);
    expect(isUniqueColumn([["1"], ["1"]], 0)).toBe(false);
    expect(isUniqueColumn([["1"], [""]], 0)).toBe(false);
  });
});

describe("diffTables", () => {
  const OLD = "id,name,qty\n1,apple,3\n2,pear,5\n3,plum,7\n";

  it("uses the first column as key when unique and marks changed cells", () => {
    const d = diffTables(sides(OLD, "id,name,qty\n1,apple,4\n3,plum,7\n4,kiwi,1\n"), AUTO);
    expect(d.header).toBe(true);
    expect(d.key).toBe(0);
    expect(d.keyUnique).toBe(true);
    expect(d.rows.map((r) => [r.kind, r.oldNo, r.newNo, r.changed])).toEqual([
      ["changed", 2, 2, [2]],
      ["removed", 3, null, []],
      ["same", 4, 3, []],
      ["added", null, 4, []],
    ]);
    expect(d.stats).toMatchObject({ same: 1, changed: 1, added: 1, removed: 1, changedCells: 1 });
    expect(cellValues(d.rows[0]!, d.columns[2]!)).toEqual({ old: "3", new: "4" });
    expect(d.noise).toBeNull();
  });

  it("pairs reordered rows by key instead of marking everything changed", () => {
    const d = diffTables(sides(OLD, "id,name,qty\n3,plum,7\n1,apple,3\n2,pear,5\n"), AUTO);
    expect(d.rows.every((r) => r.kind === "same")).toBe(true);
  });

  it("falls back to row position when the first column repeats", () => {
    const d = diffTables(sides("k,v\na,1\na,2\n", "k,v\na,1\na,3\n"), AUTO);
    expect(d.key).toBeNull();
    expect(d.rows.map((r) => r.kind)).toEqual(["same", "changed"]);
  });

  it("lets the user pick another key column, even a non-unique one", () => {
    const s = sides(OLD, "id,name,qty\n10,apple,3\n20,pear,6\n30,plum,7\n");
    const byId = diffTables(s, AUTO);
    expect(byId.stats).toMatchObject({ added: 3, removed: 3 });
    const byName = diffTables(s, { header: null, key: 1 });
    expect(byName.key).toBe(1);
    expect(byName.stats).toMatchObject({ added: 0, removed: 0, changed: 3, changedCells: 4 });
    const byPos = diffTables(s, { header: null, key: "position" });
    expect(byPos.key).toBeNull();
    const dup = diffTables(sides("a,b\nx,1\nx,2\n", "a,b\nx,1\nx,3\n"), { header: null, key: 0 });
    expect(dup.key).toBe(0);
    expect(dup.keyUnique).toBe(false);
    expect(dup.rows.map((r) => r.kind)).toEqual(["same", "changed"]);
  });

  it("reports added and removed columns without marking rows changed", () => {
    const d = diffTables(sides("id,a,b\n1,x,y\n", "id,b,c\n1,y,z\n"), AUTO);
    expect(d.columns.map((c) => [c.name, c.status])).toEqual([
      ["id", "same"],
      ["a", "removed"],
      ["b", "same"],
      ["c", "added"],
    ]);
    expect(d.stats).toMatchObject({ same: 1, changed: 0, addedColumns: 1, removedColumns: 1 });
    expect(cellValues(d.rows[0]!, d.columns[1]!)).toEqual({ old: "x", new: null });
    expect(cellValues(d.rows[0]!, d.columns[3]!)).toEqual({ old: null, new: "z" });
    // A title changed in place is a rename; a different value under it is a changed cell.
    const renamed = diffTables(sides("id,qty\n1,3\n", "id,amount\n1,4\n"), AUTO);
    expect(renamed.columns[1]).toMatchObject({ name: "amount", oldName: "qty", status: "renamed" });
    expect(renamed.stats).toMatchObject({ changed: 1, changedCells: 1, renamedColumns: 1 });
  });

  it("works without a header row", () => {
    const d = diffTables(sides("1,2\n3,4\n", "1,2\n3,5\n"), AUTO);
    expect(d.header).toBe(false);
    expect(d.rows.map((r) => [r.kind, r.oldNo])).toEqual([
      ["same", 1],
      ["changed", 2],
    ]);
    const forced = diffTables(sides("1,2\n3,4\n", "1,2\n3,5\n"), { header: true, key: "auto" });
    expect(forced.header).toBe(true);
    expect(forced.rows).toHaveLength(1);
  });

  it("shows a new or deleted file as plain rows", () => {
    const added = diffTables(sides(null, "id,v\n1,a\n2,b\n"), AUTO);
    expect(added.oneSided).toBe(true);
    expect(added.rows.map((r) => r.kind)).toEqual(["added", "added"]);
    expect(added.columns.every((c) => c.status === "same")).toBe(true);
    const removed = diffTables(sides("id,v\n1,a\n", null), AUTO);
    expect(removed.rows.map((r) => r.kind)).toEqual(["removed"]);
  });

  it("handles the TSV separator and multi-line cells", () => {
    const d = diffTables(sides('id\tnote\n1\t"a\nb"\n', 'id\tnote\n1\t"a\nc"\n', "x.tsv"), AUTO);
    expect(d.delimiter).toBe("\t");
    expect(cellValues(d.rows[0]!, d.columns[1]!)).toEqual({ old: "a\nb", new: "a\nc" });
  });
});

describe("formatNoise", () => {
  const base = "id,name\n1,apple\n2,pear\n";

  it("spots line ending, quoting, spacing and BOM only changes", () => {
    expect(formatNoise(sides(base, base.replace(/\n/g, "\r\n")))).toEqual(["lineEndings"]);
    expect(formatNoise(sides(base, 'id,name\n1,"apple"\n2,pear\n'))).toEqual(["quoting"]);
    expect(formatNoise(sides(base, "id,name\n1,apple  \n2, pear\n"))).toEqual(["whitespace"]);
    expect(formatNoise(sides(base, BOM + base))).toEqual(["bom"]);
    expect(formatNoise(sides(base, 'id,name\r\n1,"apple"\r\n2,pear'))).toEqual([
      "lineEndings",
      "quoting",
    ]);
  });

  it("is null when values change or nothing changed", () => {
    expect(formatNoise(sides(base, base))).toBeNull();
    expect(formatNoise(sides(base, base.replace("pear", "plum")))).toBeNull();
    expect(formatNoise(sides(null, base))).toBeNull();
  });

  it("does not mark rows changed when only spaces around values moved", () => {
    const d = diffTables(sides(base, "id,name\n1,apple \n2,pear\n"), AUTO);
    expect(d.noise).toEqual(["whitespace"]);
    expect(d.stats).toMatchObject({ changed: 0, same: 2 });
    const byPos = diffTables(sides(base, "id,name\n1,apple \n2,pear\n"), {
      header: null,
      key: "position",
    });
    expect(byPos.stats).toMatchObject({ changed: 0, same: 2 });
  });

  it("counts a space change when other values changed too", () => {
    const d = diffTables(sides(base, "id,name\n1,apple \n2,plum\n"), AUTO);
    expect(d.noise).toBeNull();
    expect(d.stats).toMatchObject({ changed: 2, changedCells: 2 });
  });
});

describe("size limits and display items", () => {
  it("refuses tables that are too large", () => {
    expect(parseSides("a.csv", "x".repeat(TABLE_MAX_CHARS + 1), "")).toEqual({
      ok: false,
      reason: "tooLarge",
    });
    expect(parseSides("a.csv", "a\n".repeat(60_000), "a\n")).toEqual({
      ok: false,
      reason: "tooLarge",
    });
  });

  it("collapses runs of unchanged rows", () => {
    const d = diffTables(sides("k\n1\n2\n3\n4\n", "k\n1\n2\n3\n5\n"), AUTO);
    expect(displayItems(d.rows, true).map((i) => ("hidden" in i ? i.hidden : i.row.kind))).toEqual([
      3,
      "added",
      "removed",
    ]);
    expect(displayItems(d.rows, false)).toHaveLength(5);
    expect(ROW_PAGE).toBeGreaterThan(0);
  });

  it("aligns a few thousand rows quickly", () => {
    const rows = Array.from({ length: 3000 }, (_, i) => `${i},v${i}`);
    const changed = [...rows];
    changed.splice(1500, 1, "1500,edited");
    changed.splice(10, 0, "x,inserted");
    const d = diffTables(sides(`a,b\n${rows.join("\n")}`, `a,b\n${changed.join("\n")}`), {
      header: null,
      key: "position",
    });
    expect(d.stats).toMatchObject({ added: 1, changed: 1, removed: 0, same: 2999 });
  });
});
