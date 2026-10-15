import { describe, expect, it } from "vitest";
import type { Submodule, SubmoduleInfo } from "@/ipc";
import {
  absolutePath,
  accountNoteKey,
  baseName,
  needsInit,
  outOfDate,
  pathFromUrl,
  short,
  submoduleStatus,
} from "./model";

const sub = (patch: Partial<Submodule> = {}): Submodule => ({
  name: "lib",
  path: "libs/lib",
  parent: null,
  depth: 0,
  url: "https://github.com/o/lib.git",
  branch: null,
  recorded: "a".repeat(40),
  head: "a".repeat(40),
  headBranch: null,
  state: "upToDate",
  modified: false,
  untracked: false,
  ...patch,
});

const info = (patch: Partial<Submodule> = {}): SubmoduleInfo => ({
  submodule: sub(patch),
  account: { accountId: null, source: "none", hostMismatch: false },
});

describe("submodule model", () => {
  it("maps states to labels and tones", () => {
    expect(submoduleStatus(sub())).toEqual({ key: "upToDate", tone: "ok" });
    expect(submoduleStatus(sub({ untracked: true }))).toEqual({ key: "dirty", tone: "warning" });
    expect(submoduleStatus(sub({ state: "outOfDate" })).key).toBe("outOfDate");
    expect(submoduleStatus(sub({ state: "uninitialized", head: null })).tone).toBe("muted");
    expect(submoduleStatus(sub({ state: "conflicted" })).tone).toBe("danger");
  });

  it("counts what needs init or update", () => {
    const list = [info(), info({ state: "uninitialized" }), info({ state: "outOfDate" })];
    expect(needsInit(list)).toBe(1);
    expect(outOfDate(list)).toBe(1);
  });

  it("derives names and paths", () => {
    expect(baseName("lib/my sub")).toBe("my sub");
    expect(short(null)).toBe("–");
    expect(short("0123456789abcdef")).toBe("01234567");
    expect(absolutePath("C:\\work\\app", "lib/x")).toBe("C:\\work\\app\\lib\\x");
    expect(absolutePath("C:/work/app/", "lib/x")).toBe("C:/work/app/lib/x");
    expect(pathFromUrl("https://github.com/org/lib.git")).toBe("lib");
    expect(pathFromUrl("git@host:team/tools.git/")).toBe("tools");
    expect(pathFromUrl("../sibling")).toBe("sibling");
  });

  it("explains the account choice", () => {
    expect(accountNoteKey({ accountId: "a", source: "rule", hostMismatch: false })).toBe("rule");
    expect(accountNoteKey({ accountId: "a", source: "fallback", hostMismatch: true })).toBe(
      "mismatch",
    );
  });
});
