import { describe, expect, it } from "vitest";
import type { GraphRow } from "@/ipc";
import {
  cellAt,
  cellTooltip,
  COLUMN_LIMITS,
  columnLayout,
  graphWidth,
  laneX,
  LANE_WIDTH,
  MAX_DRAWN_LANES,
  parseRefs,
} from "./draw";

describe("parseRefs", () => {
  it("classifies decorations", () => {
    expect(
      parseRefs([
        "HEAD -> refs/heads/main",
        "refs/remotes/origin/main",
        "refs/remotes/upstream/HEAD",
        "tag: refs/tags/v1.2.0",
        "refs/heads/release/1.x",
      ]),
    ).toEqual([
      { text: "main", kind: "head" },
      { text: "origin/main", kind: "remote" },
      { text: "upstream/HEAD", kind: "remote" },
      { text: "v1.2.0", kind: "tag" },
      { text: "release/1.x", kind: "branch" },
    ]);
    expect(parseRefs(["HEAD"])).toEqual([{ text: "HEAD", kind: "head" }]);
  });
});

describe("geometry", () => {
  it("places lanes at fixed spacing and caps the graph width", () => {
    expect(laneX(1) - laneX(0)).toBe(LANE_WIDTH);
    expect(graphWidth(0)).toBe(graphWidth(1));
    expect(graphWidth(500)).toBe(graphWidth(MAX_DRAWN_LANES));
    expect(graphWidth(1)).toBeGreaterThanOrEqual(120);
    expect(graphWidth(20)).toBe(20 * LANE_WIDTH + 20);
  });
});

describe("columnLayout", () => {
  it("centers the lanes in the graph column and gives the message the most room", () => {
    const c = columnLayout(1200, 2);
    expect(c.graph).toBe(graphWidth(2));
    // Two lanes (28 px) centered in a 120 px column.
    expect(c.laneOffset).toBe(Math.floor((c.graph - 2 * LANE_WIDTH) / 2));
    expect(laneX(0, c.laneOffset) + laneX(1, c.laneOffset)).toBeCloseTo(c.graph, 0);
    const message = c.authorX - c.textX;
    expect(message).toBeGreaterThan(c.author);
    expect(message).toBeGreaterThan(c.date);
    expect(c.dateX + c.date).toBe(1200);
  });

  it("shrinks then hides author, then date, without letting them outgrow the message", () => {
    const mid = columnLayout(700, 1);
    expect(mid.author).toBeGreaterThan(0);
    expect(mid.authorX - mid.textX).toBeGreaterThanOrEqual(mid.author);
    const narrow = columnLayout(520, 1);
    expect(narrow.author).toBe(0);
    expect(narrow.date).toBeGreaterThan(0);
    const tiny = columnLayout(300, 1);
    expect([tiny.author, tiny.date]).toEqual([0, 0]);
  });

  it("applies and clamps the widths the user dragged", () => {
    const c = columnLayout(1400, 1, { graph: 300, author: 260, date: 9999 });
    expect(c.graph).toBe(300);
    expect(c.textX).toBe(308);
    expect(c.author).toBe(260);
    expect(c.date).toBe(COLUMN_LIMITS.date.max);
    expect(columnLayout(1400, 1, { graph: 1 }).graph).toBe(COLUMN_LIMITS.graph.min);
  });
});

describe("cell tooltips", () => {
  const row: GraphRow = {
    index: 0,
    hash: "a".repeat(40),
    subject: "feat: a fairly long commit subject that will not fit",
    authorName: "Nguyễn Văn Hoàng",
    authorEmail: "hoang@acme.vn",
    authorTime: 1_700_000_000,
    refs: ["HEAD -> refs/heads/main"],
    isMerge: false,
    lane: 0,
    color: 0,
    segments: [],
  };
  // 7 px per character for every font.
  const measure = {
    label: (t: string) => t.length * 7,
    text: (t: string) => t.length * 7,
    date: (t: string) => t.length * 7,
  };

  it("finds the cell under the pointer", () => {
    const c = columnLayout(1200, 1);
    expect(cellAt(c, 5)).toBe("graph");
    expect(cellAt(c, c.textX + 10)).toBe("message");
    expect(cellAt(c, c.authorX + 5)).toBe("author");
    expect(cellAt(c, c.dateX + 5)).toBe("date");
  });

  it("shows only cut-off text, and the hidden author / date on the message", () => {
    const wide = columnLayout(1600, 1);
    expect(cellTooltip(row, "message", wide, measure, "en")).toBeNull();
    expect(cellTooltip(row, "author", wide, measure, "en")).toBeNull();

    const narrow = columnLayout(420, 1);
    expect(narrow.author).toBe(0);
    const tip = cellTooltip(row, "message", narrow, measure, "en")!;
    expect(tip.split("\n")[0]).toBe(row.subject);
    expect(tip).toContain("Nguyễn Văn Hoàng <hoang@acme.vn>");

    const thinAuthor = columnLayout(1400, 1, { author: 60 });
    expect(cellTooltip(row, "author", thinAuthor, measure, "en")).toBe(
      "Nguyễn Văn Hoàng <hoang@acme.vn>",
    );
  });
});
