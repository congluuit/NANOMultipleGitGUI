import { describe, expect, it } from "vitest";
import { clickSelect, kindLetter } from "./format";

const order = ["a", "b", "c", "d"];
const none = { ctrl: false, shift: false };

describe("clickSelect", () => {
  it("plain click selects one", () => {
    expect([...clickSelect(order, new Set(["a", "b"]), "a", "c", none)]).toEqual(["c"]);
  });

  it("ctrl toggles", () => {
    const s = clickSelect(order, new Set(["a"]), "a", "c", { ctrl: true, shift: false });
    expect([...s].sort()).toEqual(["a", "c"]);
    expect([...clickSelect(order, s, "c", "a", { ctrl: true, shift: false })]).toEqual(["c"]);
  });

  it("shift extends from the anchor in both directions", () => {
    expect([...clickSelect(order, new Set(["d"]), "d", "b", { ctrl: false, shift: true })]).toEqual(
      ["b", "c", "d"],
    );
    expect([...clickSelect(order, new Set(), null, "b", { ctrl: false, shift: true })]).toEqual([
      "b",
    ]);
  });
});

describe("kindLetter", () => {
  it("maps change kinds", () => {
    expect(kindLetter("untracked")).toBe("U");
    expect(kindLetter("conflicted")).toBe("!");
  });
});
