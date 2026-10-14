import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it.each([
    [0, "0 B"],
    [512, "512 B"],
    [1024, "1 KB"],
    [4812, "4.7 KB"],
    [88 * 1024 * 1024, "88 MB"],
    [312 * 1024 * 1024, "312 MB"],
    [1.8 * 1024 ** 3, "1.8 GB"],
  ])("%d → %s", (bytes, expected) => {
    expect(formatBytes(bytes)).toBe(expected);
  });

  it("handles invalid input", () => {
    expect(formatBytes(-1)).toBe("–");
    expect(formatBytes(Number.NaN)).toBe("–");
  });
});
