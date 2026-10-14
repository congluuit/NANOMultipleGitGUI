import { describe, expect, it } from "vitest";
import { diffSettings } from "./settingsSync";

describe("diffSettings", () => {
  it("returns null when nothing changed", () => {
    expect(
      diffSettings({ theme: "dark", language: "vi" }, { theme: "dark", language: "vi" }),
    ).toBeNull();
  });

  it("includes only changed fields", () => {
    expect(
      diffSettings({ theme: "dark", language: "vi" }, { theme: "light", language: "vi" }),
    ).toEqual({ theme: "light" });
    expect(
      diffSettings({ theme: "dark", language: "vi" }, { theme: "dark", language: "en" }),
    ).toEqual({ language: "en" });
  });
});
