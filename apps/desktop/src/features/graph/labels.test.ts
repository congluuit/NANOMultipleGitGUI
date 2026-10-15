import { describe, expect, it } from "vitest";
import type { GraphRow } from "@/ipc";
import {
  columnLayout,
  fitText,
  groupRefs,
  labelAt,
  parseRefs,
  refAtPoint,
  refCell,
  refLabelBoxes,
  refsTooltip,
  rowPills,
} from "./draw";

// 6 px per character.
const measure = (t: string) => t.length * 6;

describe("ref grouping", () => {
  it("merges a local branch with its remote-tracking branches into one pill", () => {
    const pills = groupRefs(
      parseRefs([
        "refs/remotes/origin/main",
        "HEAD -> refs/heads/main",
        "refs/remotes/upstream/main",
        "refs/remotes/origin/HEAD",
      ]),
    );
    expect(pills).toHaveLength(1);
    expect(pills[0]).toMatchObject({
      name: "main",
      head: true,
      local: true,
      remote: true,
      label: { text: "main", kind: "head" },
    });
    expect(pills[0]!.refs.map((r) => r.text)).toEqual([
      "main",
      "origin/main",
      "upstream/main",
      "origin/HEAD",
    ]);
  });

  it("orders HEAD, local branches, remotes, then tags", () => {
    const pills = groupRefs(
      parseRefs([
        "tag: refs/tags/v1.0",
        "refs/remotes/origin/feature/x",
        "refs/heads/x",
        "refs/heads/topic",
        "HEAD -> refs/heads/dev",
      ]),
    );
    expect(pills.map((p) => [p.name, p.local, p.remote, p.tag])).toEqual([
      ["dev", true, false, false],
      ["x", true, false, false],
      ["topic", true, false, false],
      ["origin/feature/x", false, true, false],
      ["v1.0", false, false, true],
    ]);
  });

  it("keeps a detached HEAD and lone remote HEADs as their own pills", () => {
    const pills = groupRefs(parseRefs(["HEAD", "refs/remotes/origin/HEAD"]));
    expect(pills.map((p) => [p.name, p.head, p.local, p.remote])).toEqual([
      ["HEAD", true, false, false],
      ["origin/HEAD", false, false, true],
    ]);
  });

  it("lists every ref in the tooltip", () => {
    const pills = groupRefs(
      parseRefs(["HEAD -> refs/heads/main", "refs/remotes/origin/main", "tag: refs/tags/v2"]),
    );
    expect(refsTooltip(pills)).toBe("✓ main, origin/main\ntag: v2");
    expect(refsTooltip([])).toBeNull();
  });
});

describe("ref cell layout and hit-testing", () => {
  it("shows the first pill and a +N counter", () => {
    const pills = groupRefs(
      parseRefs(["refs/heads/topic", "tag: refs/tags/v1", "tag: refs/tags/v2"]),
    );
    const cell = refCell(pills, { refs: 150 }, measure)!;
    expect(cell.pill.name).toBe("topic");
    expect(cell.name).toBe("topic");
    expect(cell.more).toBe(2);
    expect(cell.moreText).toBe("+2");
    expect(cell.moreX).toBeGreaterThan(cell.x + cell.width);
    expect(cell.end).toBe(cell.moreX + cell.moreWidth);
    expect(cell.end).toBeLessThanOrEqual(150);
  });

  it("ellipsizes a long name to the column", () => {
    const pills = groupRefs(parseRefs(["refs/heads/feature/a-very-long-branch-name-indeed"]));
    const cell = refCell(pills, { refs: 120 }, measure)!;
    expect(cell.name.endsWith("…")).toBe(true);
    expect(cell.x + cell.width).toBeLessThanOrEqual(120);
    expect(fitText("abcdef", 24, measure)).toBe("abc…");
    expect(fitText("abc", 24, measure)).toBe("abc");
  });

  it("finds the pill under the pointer (local branch of a merged pill)", () => {
    const cols = columnLayout(1200, 1);
    const pills = groupRefs(parseRefs(["refs/remotes/origin/main", "refs/heads/main"]));
    const boxes = refLabelBoxes(pills, cols, measure);
    expect(boxes).toHaveLength(1);
    const box = boxes[0]!;
    expect(labelAt(boxes, box.x + 2)).toEqual({ text: "main", kind: "branch" });
    expect(labelAt(boxes, box.x + box.width + 2)).toBeNull();

    const row = { refs: ["refs/remotes/origin/main", "refs/heads/main"] } as GraphRow;
    expect(rowPills(row)).toBe(rowPills(row));
    expect(refAtPoint(row, cols, box.x + 4, measure)).toEqual({ text: "main", kind: "branch" });
    expect(refAtPoint(row, cols, cols.refs + 10, measure)).toBeNull();
    expect(refAtPoint({ refs: [] as string[] } as GraphRow, cols, box.x + 4, measure)).toBeNull();
  });
});
