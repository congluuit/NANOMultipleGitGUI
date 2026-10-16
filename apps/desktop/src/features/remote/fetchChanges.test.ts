import { describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { LocalBranch, RepoRefs } from "@/ipc";
import { diffRefs, fetchChangesNotice } from "./fetchChanges";

initI18n("en");

const local = (name: string, behind: number, isHead = false): LocalBranch => ({
  name,
  oid: "a",
  upstream: `origin/${name}`,
  ahead: 0,
  behind,
  upstreamGone: false,
  isHead,
});

const refs = (branches: Record<string, string>, locals: LocalBranch[] = []): RepoRefs => ({
  head: { branch: "main", oid: "a", detached: false, unborn: false },
  local: locals,
  remotes: [
    {
      name: "origin",
      url: null,
      branches: Object.entries(branches).map(([name, oid]) => ({ name, oid })),
    },
  ],
  tags: [],
  stashes: [],
});

describe("diffRefs", () => {
  it("finds moved, new and removed remote branches and branches now behind", () => {
    const before = refs({ HEAD: "1", main: "1", old: "2", same: "3" }, [local("main", 0, true)]);
    const after = refs({ HEAD: "9", main: "4", same: "3", "feature/x": "5" }, [
      local("main", 2, true),
    ]);
    expect(diffRefs(before, after)).toEqual({
      updated: ["origin/main"],
      added: ["origin/feature/x"],
      removed: ["origin/old"],
      behind: [{ branch: "main", count: 2, isHead: true }],
    });
  });

  it("reports nothing when nothing moved", () => {
    const r = refs({ main: "1" }, [local("main", 1, true)]);
    expect(fetchChangesNotice(diffRefs(r, r))).toBeNull();
  });
});

describe("fetchChangesNotice", () => {
  it("offers a Pull for the current branch when it is behind", () => {
    const pull = vi.fn();
    const notice = fetchChangesNotice(
      {
        updated: ["origin/main"],
        added: [],
        removed: [],
        behind: [{ branch: "main", count: 3, isHead: true }],
      },
      pull,
    );
    expect(notice?.message).toContain("1");
    expect(notice?.hint).toContain("main");
    notice?.actions?.[0]?.run();
    expect(pull).toHaveBeenCalled();
  });

  it("caps the list of lines", () => {
    const many = Array.from({ length: 10 }, (_, i) => `origin/b${i}`);
    const notice = fetchChangesNotice({ updated: [], added: many, removed: [], behind: [] });
    expect(notice?.hint?.split("\n")).toHaveLength(7);
    expect(notice?.actions).toEqual([]);
  });
});
