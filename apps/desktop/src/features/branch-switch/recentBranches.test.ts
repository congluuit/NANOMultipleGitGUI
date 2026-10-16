import { describe, expect, it } from "vitest";
import { pushRecent, RECENT_BRANCHES_LIMIT, recentChoices } from "./recentBranches";

describe("pushRecent", () => {
  it("puts the new branch first and the one we came from second", () => {
    expect(pushRecent(["a", "b", "c"], "b", "d")).toEqual(["d", "b", "a", "c"]);
    expect(pushRecent([], null, "main")).toEqual(["main"]);
    expect(pushRecent(["main"], "main", "main")).toEqual(["main"]);
  });

  it("caps the list", () => {
    const many = Array.from({ length: 20 }, (_, i) => `b${i}`);
    expect(pushRecent(many, null, "x")).toHaveLength(RECENT_BRANCHES_LIMIT);
  });
});

describe("recentChoices", () => {
  it("skips the current branch and branches deleted since", () => {
    expect(recentChoices(["main", "gone", "topic"], "main", ["main", "topic"])).toEqual(["topic"]);
  });
});
