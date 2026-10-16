import type { RecentRepo, RepoTab } from "@/ipc";

export interface RecentCandidate {
  id: number;
  name: string;
  path: string;
}

/**
 * What the "+" menu offers under "Recent repositories": the tabs closed in this session first
 * (newest first), then the other recent repositories. Never one that is already open, and never
 * a folder that no longer exists.
 */
export function recentCandidates(
  recentlyClosed: readonly RepoTab[],
  recent: readonly RecentRepo[],
  open: readonly RepoTab[],
  limit = 15,
): RecentCandidate[] {
  const openIds = new Set(open.map((tab) => tab.id));
  const gone = new Set(recent.filter((r) => !r.exists).map((r) => r.id));
  const out: RecentCandidate[] = [];
  const seen = new Set<number>();
  const add = (r: RecentCandidate) => {
    if (openIds.has(r.id) || gone.has(r.id) || seen.has(r.id)) return;
    seen.add(r.id);
    out.push({ id: r.id, name: r.name, path: r.path });
  };
  for (const tab of recentlyClosed) add(tab);
  for (const r of recent) add(r);
  return out.slice(0, limit);
}
