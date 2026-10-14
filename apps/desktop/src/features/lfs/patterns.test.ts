import { describe, expect, it } from "vitest";
import type { LfsStatus } from "@/ipc";
import { canUntrack, isValidPattern, lfsVersion, shouldOfferInstall } from "./patterns";

const status = (over: Partial<LfsStatus> = {}): LfsStatus => ({
  gitLfs: {
    version: { major: 3, minor: 6, patch: 1 },
    raw: "git-lfs/3.6.1",
    minimum: { major: 3, minor: 0, patch: 0 },
    ok: true,
  },
  patterns: [],
  usesLfs: false,
  hooksDir: ".git/hooks",
  hooks: [],
  hooksInstalled: false,
  filterConfigured: true,
  needsInstall: false,
  ...over,
});

describe("isValidPattern", () => {
  it("accepts ordinary patterns", () => {
    for (const p of ["*.psd", "assets/video/**", "/build/app.zip", "My Files/*.png", "[*].bin"]) {
      expect(isValidPattern(p), p).toBe(true);
    }
  });

  it("rejects what the backend rejects", () => {
    for (const p of ["", " *.psd", "*.psd ", "-x", "#comment", "!negated", 'a"b', "a\nb", "a\tb"]) {
      expect(isValidPattern(p), JSON.stringify(p)).toBe(false);
    }
    expect(isValidPattern("a".repeat(4097))).toBe(false);
  });
});

describe("canUntrack", () => {
  it("only for the root .gitattributes", () => {
    const p = { pattern: "*.psd", line: 1, lockable: false };
    expect(canUntrack({ ...p, file: ".gitattributes" })).toBe(true);
    expect(canUntrack({ ...p, file: "assets/.gitattributes" })).toBe(false);
    expect(canUntrack({ ...p, file: "$GIT_DIR/info/attributes" })).toBe(false);
  });
});

describe("shouldOfferInstall", () => {
  it("follows the backend flag", () => {
    expect(shouldOfferInstall(status({ needsInstall: true }))).toBe(true);
    expect(shouldOfferInstall(status())).toBe(false);
  });

  it("also offers it when patterns exist but hooks are missing", () => {
    expect(shouldOfferInstall(status({ usesLfs: true, hooksInstalled: false }))).toBe(true);
    expect(shouldOfferInstall(status({ usesLfs: true, hooksInstalled: true }))).toBe(false);
  });

  it("never without git-lfs", () => {
    const missing = status({ needsInstall: true, usesLfs: true });
    missing.gitLfs = { ...missing.gitLfs, version: null, raw: null, ok: false };
    expect(shouldOfferInstall(missing)).toBe(false);
    expect(lfsVersion(missing)).toBeNull();
  });
});

describe("lfsVersion", () => {
  it("formats the parsed version", () => {
    expect(lfsVersion(status())).toBe("3.6.1");
  });
});
