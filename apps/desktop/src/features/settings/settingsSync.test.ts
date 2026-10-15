import { describe, expect, it } from "vitest";
import type { SettingsValues } from "@/stores/settings";
import { diffSettings } from "./settingsSync";

const base: SettingsValues = {
  theme: "dark",
  language: "vi",
  ciEnabled: false,
  authorAvatars: true,
  uiScale: 1,
  confirmBranchSwitch: true,
};

describe("diffSettings", () => {
  it("returns null when nothing changed", () => {
    expect(diffSettings(base, { ...base })).toBeNull();
  });

  it("includes only changed fields", () => {
    expect(diffSettings(base, { ...base, theme: "light" })).toEqual({ theme: "light" });
    expect(diffSettings(base, { ...base, language: "en" })).toEqual({ language: "en" });
    expect(diffSettings(base, { ...base, ciEnabled: true })).toEqual({ ciEnabled: true });
    expect(diffSettings(base, { ...base, authorAvatars: false })).toEqual({
      authorAvatars: false,
    });
  });
});
