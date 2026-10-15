import { describe, expect, it } from "vitest";
import type { LfsFile, LfsLock } from "@/ipc";
import {
  downloadable,
  fileState,
  fileTotals,
  filterFiles,
  groupProblems,
  lockBadge,
  locksByPath,
} from "./lfsModel";

const file = (path: string, patch: Partial<LfsFile> = {}): LfsFile => ({
  path,
  oid: "a".repeat(64),
  size: 100,
  downloaded: true,
  checkedOut: true,
  lockable: false,
  ...patch,
});

const lock = (path: string, ours: boolean, owner: string | null = "bob"): LfsLock => ({
  id: path,
  path,
  owner,
  lockedAt: null,
  ours,
});

describe("lfs model", () => {
  it("classifies files", () => {
    expect(fileState(file("a"))).toBe("downloaded");
    expect(fileState(file("a", { checkedOut: false }))).toBe("pointer");
    expect(fileState(file("a", { checkedOut: false, downloaded: false }))).toBe("missing");
  });

  it("filters, totals and picks what to download", () => {
    const files = [
      file("art/a.psd", { checkedOut: false, downloaded: false, size: 300 }),
      file("b.bin"),
      file("art/c.psd", { checkedOut: false }),
    ];
    const locks = locksByPath({ locks: [lock("b.bin", false)], verified: true });
    expect(
      filterFiles(files, locks, { text: "ART", pointersOnly: false, lockedOnly: false }),
    ).toHaveLength(2);
    expect(
      filterFiles(files, locks, { text: "", pointersOnly: true, lockedOnly: false }),
    ).toHaveLength(2);
    expect(
      filterFiles(files, locks, { text: "", pointersOnly: false, lockedOnly: true })[0]!.path,
    ).toBe("b.bin");
    expect(fileTotals(files)).toEqual({ count: 3, bytes: 500, pointers: 2 });
    expect(downloadable(files, new Set(["b.bin", "art/c.psd"]))).toEqual(["art/c.psd"]);
  });

  it("builds lock badges", () => {
    expect(lockBadge(undefined)).toBeNull();
    expect(lockBadge(lock("x", true))).toEqual({ owner: "bob", ours: true });
    expect(lockBadge(lock("x", false, null))).toEqual({ owner: "?", ours: false });
    expect(locksByPath(null).size).toBe(0);
  });

  it("groups fsck problems by kind", () => {
    const g = groupProblems([
      { kind: "missing", path: "a", oid: null, detail: "1" },
      { kind: "corrupt", path: "b", oid: null, detail: "2" },
      { kind: "missing", path: "c", oid: null, detail: "3" },
    ]);
    expect(g.get("missing")).toHaveLength(2);
    expect([...g.keys()]).toEqual(["missing", "corrupt"]);
  });
});
