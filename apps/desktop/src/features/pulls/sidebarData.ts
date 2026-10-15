import type { HostedRemote, PullListQuery } from "@/ipc";
import { useHostedRemotes, usePullList } from "./queries";
import { usePullFilters, type PullFilters } from "./store";

/** Hosted remote the sidebar lists (the chosen one, else `origin` / the first). */
export function pickRemote(
  remotes: readonly HostedRemote[] | undefined,
  name: string | null,
): HostedRemote | null {
  if (!remotes?.length) return null;
  return (name ? remotes.find((r) => r.remote === name) : null) ?? remotes[0] ?? null;
}

export function toQuery(f: PullFilters): PullListQuery {
  const v = (s: string) => s.trim() || null;
  return { state: f.state, author: v(f.author), reviewer: v(f.reviewer), label: v(f.label) };
}

/** Listing state shared by the sidebar section and its count. */
export function useSidebarPulls(repoId: number) {
  const filters = usePullFilters(repoId);
  const remotes = useHostedRemotes(repoId);
  const remote = pickRemote(remotes.data, filters.remote);
  const usable = !!remote?.accountId && remote.hasToken;
  const list = usePullList(
    usable ? remote.accountId : null,
    usable ? remote.fullName : null,
    toQuery(filters),
  );
  return { filters, remotes, remote, usable, list };
}

/** Number shown next to "PR / MR" in the sidebar (`undefined` while unknown). */
export function usePullSidebarCount(repoId: number | null | undefined): number | undefined {
  const { list, usable } = useSidebarPulls(repoId ?? -1);
  if (repoId == null || !usable || !list.data) return undefined;
  const first = list.data.pages[0];
  return first?.total ?? list.data.pages.reduce((n, p) => n + p.items.length, 0);
}
