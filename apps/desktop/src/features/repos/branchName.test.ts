import { describe, expect, it } from "vitest";
import { isPlausibleBranchName } from "./branchName";

describe("isPlausibleBranchName", () => {
  it.each(["main", "master", "develop", "feature/login", "release-1.2", "nhánh-chính"])(
    "accepts %s",
    (name) => expect(isPlausibleBranchName(name)).toBe(true),
  );
  it.each([
    "",
    "  ",
    "has space",
    "a..b",
    "-x",
    "/x",
    "x/",
    "x.lock",
    "x.",
    "a:b",
    "a~1",
    "a^",
    "a@{1}",
  ])("rejects %j", (name) => expect(isPlausibleBranchName(name)).toBe(false));
});
