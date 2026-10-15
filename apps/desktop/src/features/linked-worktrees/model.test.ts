import { describe, expect, it } from "vitest";
import type { Worktree } from "@/ipc";
import {
  checkedOutBranches,
  folderName,
  parentFolder,
  samePath,
  slug,
  suggestFolder,
  worktreeLabel,
} from "./model";

const wt = (patch: Partial<Worktree>): Worktree => ({
  path: "C:/work/app",
  head: "0123456789abcdef",
  branch: "main",
  detached: false,
  bare: false,
  isMain: true,
  isCurrent: true,
  locked: false,
  lockReason: null,
  prunable: false,
  prunableReason: null,
  ...patch,
});

describe("linked worktree model", () => {
  it("suggests a folder next to the main worktree", () => {
    expect(suggestFolder("C:/work/app", "feature/login")).toBe("C:/work/app-feature-login");
    expect(suggestFolder("C:\\work\\app", "fix")).toBe("C:\\work\\app-fix");
    expect(suggestFolder("C:/work/app", "")).toBe("C:/work/app");
    expect(slug(" a b:c ")).toBe("a-b-c");
    expect(folderName("C:\\work\\app-x\\")).toBe("app-x");
    expect(parentFolder("C:/work/app/")).toBe("C:/work");
  });

  it("compares paths like Windows does", () => {
    expect(samePath("C:\\Work\\App\\", "c:/work/app")).toBe(true);
    expect(samePath("C:/a", "C:/b")).toBe(false);
  });

  it("labels and indexes checked-out branches", () => {
    expect(worktreeLabel(wt({}))).toBe("main");
    expect(worktreeLabel(wt({ branch: null, detached: true }))).toBe("01234567");
    const map = checkedOutBranches([
      wt({}),
      wt({ path: "C:/work/app-x", branch: "x", isMain: false, isCurrent: false }),
      wt({ path: "C:/work/app-d", branch: null, detached: true }),
    ]);
    expect([...map.keys()]).toEqual(["main", "x"]);
    expect(map.get("x")).toBe("C:/work/app-x");
  });
});
