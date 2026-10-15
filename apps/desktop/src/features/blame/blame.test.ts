import { describe, expect, it } from "vitest";
import {
  AGE_BUCKETS,
  ageBucket,
  ageOpacity,
  groupIndex,
  groupLines,
  relativeTime,
  timeRange,
  visibleRange,
} from "./blame";

describe("groupLines", () => {
  it("merges consecutive lines of the same commit only", () => {
    const groups = groupLines([0, 0, 1, 1, 1, 0, 2]);
    expect(groups).toEqual([
      { start: 0, count: 2, commit: 0 },
      { start: 2, count: 3, commit: 1 },
      { start: 5, count: 1, commit: 0 },
      { start: 6, count: 1, commit: 2 },
    ]);
    expect(Array.from(groupIndex(groups, 7))).toEqual([0, 0, 1, 1, 1, 2, 3]);
    expect(groupLines([])).toEqual([]);
  });
});

describe("age colors", () => {
  it("spreads commits from newest to oldest", () => {
    const newest = 1_000_000_000;
    const oldest = newest - 10 * 365 * 24 * 3600;
    expect(ageBucket(newest, newest, oldest)).toBe(0);
    expect(ageBucket(oldest, newest, oldest)).toBe(AGE_BUCKETS - 1);
    const month = ageBucket(newest - 30 * 24 * 3600, newest, oldest);
    const years = ageBucket(newest - 5 * 365 * 24 * 3600, newest, oldest);
    expect(month).toBeGreaterThan(0);
    expect(years).toBeGreaterThan(month);
    // A single-commit file is all "new".
    expect(ageBucket(5, 5, 5)).toBe(0);
    expect(ageOpacity(0)).toBe(1);
    expect(ageOpacity(AGE_BUCKETS - 1)).toBeCloseTo(0.2);
  });

  it("ignores uncommitted lines for the time range", () => {
    expect(
      timeRange([
        { time: 50, uncommitted: false },
        { time: 999, uncommitted: true },
        { time: 10, uncommitted: false },
      ]),
    ).toEqual({ newest: 50, oldest: 10 });
    expect(timeRange([])).toEqual({ newest: 0, oldest: 0 });
  });
});

describe("relativeTime", () => {
  it("picks the largest unit", () => {
    const now = 1_000_000;
    expect(relativeTime(now - 3 * 24 * 3600, now, "en")).toBe("3 days ago");
    expect(relativeTime(now - 2 * 3600, now, "en")).toBe("2 hours ago");
    expect(relativeTime(now - 10, now, "en")).toBe("this minute");
    expect(relativeTime(now - 400 * 24 * 3600, now, "vi")).toContain("năm");
  });
});

describe("visibleRange", () => {
  it("renders the window with overscan, clamped", () => {
    expect(visibleRange(0, 180, 18, 1000, 5)).toEqual({ first: 0, last: 15 });
    expect(visibleRange(1800, 180, 18, 1000, 5)).toEqual({ first: 95, last: 115 });
    expect(visibleRange(18_000, 180, 18, 1000, 5)).toEqual({ first: 995, last: 1000 });
  });
});
