import { describe, expect, it } from "vitest";
import { detectLanguage, translate } from "./i18n";
import { folderError, stageProgress } from "./model";

describe("stageProgress", () => {
  it("moves forward through the stages and never reaches 100 before the end", () => {
    expect(stageProgress(null, 0)).toBe(0);
    expect(stageProgress("extracting", 0)).toBeLessThan(stageProgress("installing", 0));
    expect(stageProgress("installing", 1_000)).toBeLessThan(stageProgress("installing", 10_000));
    expect(stageProgress("installing", 10 * 60_000)).toBeLessThanOrEqual(90);
    expect(stageProgress("finishing", 0)).toBeLessThan(100);
  });
});

describe("folderError", () => {
  it("recognizes only the folder codes", () => {
    expect(folderError("notWritable")).toBe("notWritable");
    expect(folderError("exit:2")).toBeNull();
    expect(folderError(null)).toBeNull();
  });
});

describe("i18n", () => {
  it("detects Vietnamese and fills parameters", () => {
    expect(detectLanguage("vi-VN")).toBe("vi");
    expect(detectLanguage("en-US")).toBe("en");
    expect(detectLanguage(undefined)).toBe("en");
    expect(translate("en", "sidebar.version", { version: "1.2.3" })).toBe("Version 1.2.3");
  });
});
