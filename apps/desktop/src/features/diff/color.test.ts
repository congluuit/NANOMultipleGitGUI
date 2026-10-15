import { describe, expect, it } from "vitest";
import { expandHex } from "./color";

describe("expandHex", () => {
  it("expands short hex colors and leaves others alone", () => {
    expect(expandHex("#fff")).toBe("#ffffff");
    expect(expandHex("#A3c8")).toBe("#AA33cc88");
    expect(expandHex("#1f1b16")).toBe("#1f1b16");
    expect(expandHex("rgb(1, 2, 3)")).toBe("rgb(1, 2, 3)");
    expect(expandHex("")).toBe("");
  });
});
