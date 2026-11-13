import { describe, expect, it } from "vitest";
import { canGroupByAccount, groupByAccount } from "./tabGroups";

const accounts: Record<number, string | null> = {
  1: "work",
  2: "home",
  3: "work",
  4: null,
  5: "home",
  6: "work",
};
const accountOf = (id: number) => accounts[id] ?? null;

describe("group tabs by account", () => {
  it("puts tabs of one account together, unbound tabs last", () => {
    expect(groupByAccount([1, 2, 3, 4, 5, 6], [], accountOf)).toEqual([1, 3, 6, 2, 5, 4]);
  });

  it("leaves pinned tabs where they are", () => {
    expect(groupByAccount([2, 1, 4, 3, 5], [2], accountOf)).toEqual([2, 1, 3, 5, 4]);
    expect(groupByAccount([1, 4, 2, 3], [4], accountOf)).toEqual([1, 4, 3, 2]);
  });

  it("knows when there is nothing to do", () => {
    expect(canGroupByAccount([1, 3, 2, 5, 4], [], accountOf)).toBe(false);
    expect(canGroupByAccount([1, 2, 3], [], accountOf)).toBe(true);
    expect(canGroupByAccount([], [], accountOf)).toBe(false);
  });
});
