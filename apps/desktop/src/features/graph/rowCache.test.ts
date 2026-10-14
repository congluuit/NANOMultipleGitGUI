import { describe, expect, it, vi } from "vitest";
import { RowCache } from "./rowCache";

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("RowCache", () => {
  it("fetches only missing pages, once each", async () => {
    const fetch = vi.fn(async (offset: number, limit: number) =>
      Array.from({ length: limit }, (_, i) => offset + i),
    );
    const cache = new RowCache(fetch, 1000, 100);
    const onLoad = vi.fn();

    cache.ensure(150, 320, onLoad);
    cache.ensure(150, 320, onLoad); // in flight: no duplicate requests
    expect(fetch.mock.calls).toEqual([
      [100, 100],
      [200, 100],
      [300, 100],
    ]);
    await tick();
    expect(onLoad).toHaveBeenCalledTimes(3);
    expect(cache.get(150)).toBe(150);
    expect(cache.get(399)).toBe(399);
    expect(cache.get(400)).toBeUndefined();
    expect(cache.has(100, 400)).toBe(true);
    expect(cache.has(100, 401)).toBe(false);

    cache.ensure(120, 380, onLoad);
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("clamps to the available range", () => {
    const fetch = vi.fn(async (_offset: number, _limit: number) => [] as number[]);
    const cache = new RowCache(fetch, 250, 100);
    cache.ensure(-50, 10_000, () => {});
    expect(fetch.mock.calls.map((c) => c[0])).toEqual([0, 100, 200]);
  });
});
