import { describe, expect, it } from "vitest";
import { graphWidth, laneX, LANE_WIDTH, MAX_DRAWN_LANES, parseRefs } from "./draw";

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
