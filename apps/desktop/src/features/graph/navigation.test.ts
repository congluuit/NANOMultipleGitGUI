import { describe, expect, it } from "vitest";
import { ROW_HEIGHT } from "./draw";
import { cycleMatch, nextIndex, scrollTopFor } from "./navigation";

describe("scrollTopFor", () => {
  const h = ROW_HEIGHT * 10;
  it("aligns to top and center", () => {
    expect(scrollTopFor(50, "top", 0, h)).toBe(50 * ROW_HEIGHT);
    expect(scrollTopFor(50, "center", 0, h)).toBe(50 * ROW_HEIGHT - h / 2 + ROW_HEIGHT / 2);
    expect(scrollTopFor(1, "center", 0, h)).toBe(0);
  });

  it("scrolls only as needed for nearest", () => {
    expect(scrollTopFor(5, "nearest", 0, h)).toBeNull();
    expect(scrollTopFor(10, "nearest", 0, h)).toBe(ROW_HEIGHT);
    expect(scrollTopFor(2, "nearest", 5 * ROW_HEIGHT, h)).toBe(2 * ROW_HEIGHT);
  });
});

describe("nextIndex", () => {
  it("moves and clamps", () => {
    expect(nextIndex("ArrowDown", null, 10, 5)).toBe(0);
    expect(nextIndex("ArrowDown", 9, 10, 5)).toBe(9);
    expect(nextIndex("ArrowUp", 0, 10, 5)).toBe(0);
    expect(nextIndex("PageDown", 3, 10, 5)).toBe(8);
    expect(nextIndex("PageUp", 3, 10, 5)).toBe(0);
    expect(nextIndex("Home", 7, 10, 5)).toBe(0);
    expect(nextIndex("End", 0, 10, 5)).toBe(9);
    expect(nextIndex("a", 0, 10, 5)).toBeNull();
    expect(nextIndex("ArrowDown", null, 0, 5)).toBeNull();
  });
});

describe("cycleMatch", () => {
  it("wraps in both directions", () => {
    expect(cycleMatch(2, 3, false)).toBe(0);
    expect(cycleMatch(0, 3, true)).toBe(2);
    expect(cycleMatch(0, 0, false)).toBe(-1);
  });
});
