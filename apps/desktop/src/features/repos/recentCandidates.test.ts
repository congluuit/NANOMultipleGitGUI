import { describe, expect, it } from "vitest";
import type { RecentRepo, RepoTab } from "@/ipc";
import { recentCandidates } from "./recentCandidates";

const tab = (id: number, name = `repo${id}`): RepoTab => ({
  id,
  name,
  path: `D:/w/${name}`,
  missing: false,
});
const recent = (id: number, exists = true): RecentRepo => ({
  ...tab(id),
  lastOpenedAt: 1000 - id,
  exists,
});

describe("recentCandidates", () => {
  it("lists the tabs closed in this session first, then the other recent repositories", () => {
    const out = recentCandidates(
      [tab(5), tab(3)],
      [recent(1), recent(3), recent(5), recent(7)],
      [],
    );
    expect(out.map((c) => c.id)).toEqual([5, 3, 1, 7]);
  });

  it("leaves out open tabs and folders that no longer exist", () => {
    const out = recentCandidates(
      [tab(5), tab(3)],
      [recent(1), recent(3, false), recent(5), recent(7)],
      [tab(5), tab(9)],
    );
    expect(out.map((c) => c.id)).toEqual([1, 7]);
  });

  it("caps the list", () => {
    const many = Array.from({ length: 30 }, (_, i) => recent(i + 1));
    expect(recentCandidates([], many, [], 15)).toHaveLength(15);
  });
});
