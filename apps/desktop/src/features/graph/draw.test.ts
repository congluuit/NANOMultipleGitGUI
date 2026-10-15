import { describe, expect, it } from "vitest";
import type { GraphRow } from "@/ipc";
import {
  cellAt,
  cellTooltip,
  COLUMN_LIMITS,
  columnLayout,
  drawGraph,
  graphWidth,
  laneX,
  LANE_WIDTH,
  MAX_DRAWN_LANES,
  parseRefs,
  ROW_HEIGHT,
  type Palette,
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
    expect(graphWidth(1)).toBeGreaterThanOrEqual(80);
    expect(graphWidth(20)).toBe(20 * LANE_WIDTH + 20);
  });
});

describe("columnLayout", () => {
  it("puts the Branch / Tag column first and centers the lanes in the graph column", () => {
    const c = columnLayout(1200, 2);
    expect(c.refs).toBe(150);
    expect(c.graphX).toBe(c.refs);
    expect(c.graph).toBe(graphWidth(2));
    expect(c.textX).toBe(c.graphX + c.graph + 8);
    // Two lanes centered in the graph column.
    expect(c.laneOffset).toBe(c.graphX + Math.floor((c.graph - 2 * LANE_WIDTH) / 2));
    expect(laneX(0, c.laneOffset) + laneX(1, c.laneOffset)).toBeCloseTo(2 * c.graphX + c.graph, 0);
    const message = c.authorX - c.textX;
    expect(message).toBeGreaterThan(c.author);
    expect(message).toBeGreaterThan(c.date);
    expect(c.dateX + c.date).toBe(1200);
  });

  it("shrinks then hides author, then date, without letting them outgrow the message", () => {
    const mid = columnLayout(700, 1);
    expect(mid.author).toBeGreaterThan(0);
    expect(mid.authorX - mid.textX).toBeGreaterThanOrEqual(mid.author);
    const narrow = columnLayout(600, 1);
    expect(narrow.author).toBe(0);
    expect(narrow.date).toBeGreaterThan(0);
    const tiny = columnLayout(300, 1);
    expect([tiny.author, tiny.date]).toEqual([0, 0]);
  });

  it("applies and clamps the widths the user dragged", () => {
    const c = columnLayout(1400, 1, { refs: 200, graph: 300, author: 260, date: 9999 });
    expect(c.refs).toBe(200);
    expect(c.graph).toBe(300);
    expect(c.textX).toBe(508);
    expect(c.author).toBe(260);
    expect(c.date).toBe(COLUMN_LIMITS.date.max);
    expect(columnLayout(1400, 1, { graph: 1 }).graph).toBe(COLUMN_LIMITS.graph.min);
    expect(columnLayout(1400, 1, { refs: 9999 }).refs).toBe(COLUMN_LIMITS.refs.max);
    expect(columnLayout(1400, 1, { refs: 1 }).refs).toBe(COLUMN_LIMITS.refs.min);
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
    expect(cellAt(c, 5)).toBe("refs");
    expect(cellAt(c, c.graphX + 5)).toBe("graph");
    expect(cellAt(c, c.textX + 10)).toBe("message");
    expect(cellAt(c, c.authorX + 5)).toBe("author");
    expect(cellAt(c, c.dateX + 5)).toBe("date");
  });

  it("shows only cut-off text, and the hidden author / date on the message", () => {
    const wide = columnLayout(1600, 1);
    expect(cellTooltip(row, "message", wide, measure, "en")).toBeNull();
    expect(cellTooltip(row, "author", wide, measure, "en")).toBeNull();

    const narrow = columnLayout(560, 1);
    expect(narrow.author).toBe(0);
    const tip = cellTooltip(row, "message", narrow, measure, "en")!;
    expect(tip.split("\n")[0]).toBe(row.subject);
    expect(tip).toContain("Nguyễn Văn Hoàng <hoang@acme.vn>");
    // Ref labels moved out of the message: they do not appear there any more.
    expect(tip).not.toContain("main");

    const thinAuthor = columnLayout(1400, 1, { author: 60 });
    expect(cellTooltip(row, "author", thinAuthor, measure, "en")).toBe(
      "Nguyễn Văn Hoàng <hoang@acme.vn>",
    );
  });

  it("lists every ref on the Branch / Tag cell", () => {
    const c = columnLayout(1200, 1);
    const tagged = {
      ...row,
      refs: ["HEAD -> refs/heads/main", "refs/remotes/origin/main", "tag: refs/tags/v1"],
    };
    expect(cellTooltip(tagged, "refs", c, measure, "en")).toBe("✓ main, origin/main\ntag: v1");
    expect(cellTooltip({ ...row, refs: [] }, "refs", c, measure, "en")).toBeNull();
  });
});

/** Canvas context that records the calls the draw code makes. */
function recordingContext() {
  const calls: { name: string; args: unknown[] }[] = [];
  const state: Record<string | symbol, unknown> = {};
  const ctx = new Proxy(state, {
    get(target, key) {
      if (key in target) return target[key];
      if (key === "measureText") return (t: string) => ({ width: t.length * 6 });
      return (...args: unknown[]) => calls.push({ name: String(key), args });
    },
    set(target, key, value) {
      target[key] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
  return { ctx, calls };
}

describe("drawGraph", () => {
  const palette: Palette = {
    lanes: ["#d9480f", "#1971c2", "#7048e8", "#2f9e44", "#c2255c", "#b35f00", "#5c940d", "#c92a2a"],
    bg: "#faf8f5",
    surface2: "#f2eee8",
    primary: "#c2410c",
    text: "#1f1b16",
    muted: "#6b635a",
    border: "#e4ded5",
    font: "sans-serif",
    mono: "monospace",
  };
  const base: GraphRow = {
    index: 0,
    hash: "b".repeat(40),
    subject: "fix: something",
    authorName: "Ann Lee",
    authorEmail: "ann@acme.vn",
    authorTime: 1_700_000_000,
    refs: ["HEAD -> refs/heads/main", "refs/remotes/origin/main", "tag: refs/tags/v1"],
    isMerge: false,
    lane: 0,
    color: 1,
    segments: [],
  };
  const merge: GraphRow = { ...base, index: 1, refs: [], isMerge: true, subject: "Merge x" };

  it("draws the ref pill, the +N counter, avatars or initials, and the subject", () => {
    const { ctx, calls } = recordingContext();
    const img = {} as HTMLImageElement;
    const avatar = (email: string) => (email === "ann@acme.vn" ? img : null);
    const missing = drawGraph({
      ctx,
      width: 1200,
      height: ROW_HEIGHT * 3,
      scrollTop: 0,
      total: 3,
      maxLanes: 1,
      getRow: (i) => [base, merge][i],
      palette,
      selected: 0,
      hovered: null,
      locale: "en",
      avatar,
    });
    expect(missing).toBe(1);
    const texts = calls.filter((c) => c.name === "fillText").map((c) => c.args[0]);
    expect(texts).toEqual(expect.arrayContaining(["main", "+1", "fix: something", "Merge x"]));
    // The author with an avatar gets the image, not the initials.
    expect(calls.filter((c) => c.name === "drawImage")).toHaveLength(1);
    expect(texts).not.toContain("AL");

    const second = recordingContext();
    drawGraph({
      ctx: second.ctx,
      width: 1200,
      height: ROW_HEIGHT,
      scrollTop: 0,
      total: 1,
      maxLanes: 1,
      getRow: () => base,
      palette,
      selected: null,
      hovered: null,
      locale: "en",
    });
    const texts2 = second.calls.filter((c) => c.name === "fillText").map((c) => c.args[0]);
    expect(texts2).toContain("AL");
    expect(second.calls.filter((c) => c.name === "drawImage")).toHaveLength(0);
  });
});
