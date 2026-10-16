import { describe, expect, it } from "vitest";
import { moveTab, orderTabs } from "./pinnedTabs";

const tabs = [1, 2, 3, 4].map((id) => ({ id }));

describe("orderTabs", () => {
  it("shows pinned tabs first, keeping the workspace order in each group", () => {
    expect(orderTabs(tabs, [3, 1]).map((t) => t.id)).toEqual([1, 3, 2, 4]);
    expect(orderTabs(tabs, []).map((t) => t.id)).toEqual([1, 2, 3, 4]);
  });
});

describe("moveTab", () => {
  it("drops a tab before or after another", () => {
    expect(moveTab([1, 2, 3, 4], [], 1, 3, false)).toEqual([2, 1, 3, 4]);
    expect(moveTab([1, 2, 3, 4], [], 1, 3, true)).toEqual([2, 3, 1, 4]);
    expect(moveTab([1, 2, 3, 4], [], 4, 1, false)).toEqual([4, 1, 2, 3]);
  });

  it("does not move across the pinned / unpinned boundary", () => {
    expect(moveTab([1, 2, 3], [1], 2, 1, false)).toEqual([1, 2, 3]);
    expect(moveTab([1, 2, 3], [1], 1, 3, true)).toEqual([1, 2, 3]);
  });

  it("ignores unknown tabs and a drop on itself", () => {
    expect(moveTab([1, 2], [], 1, 1, true)).toEqual([1, 2]);
    expect(moveTab([1, 2], [], 9, 1, true)).toEqual([1, 2]);
  });
});
