import type { HostedRemote, WorkAccount, WorkItem, WorkLists, WorkOverview } from "@/ipc";
import { sameProject } from "@/features/issues/model";

export type WorkGroup = keyof WorkLists;

export const GROUPS: readonly WorkGroup[] = ["reviewRequested", "authored", "assigned"];

/** A row of the panel with the account it was found with. */
export interface WorkRow {
  accountId: string;
  item: WorkItem;
}

/** Rows of one group across accounts (optionally one account), most recently updated first. */
export function groupRows(
  accounts: readonly WorkAccount[],
  group: WorkGroup,
  accountId: string | null,
): WorkRow[] {
  const rows: WorkRow[] = [];
  for (const a of accounts) {
    if (accountId && a.accountId !== accountId) continue;
    for (const item of a.lists?.[group].items ?? []) rows.push({ accountId: a.accountId, item });
  }
  return rows.sort((x, y) => (y.item.updatedAt ?? 0) - (x.item.updatedAt ?? 0));
}

/** Total of a group (provider totals when known, else the rows listed). */
export function groupTotal(
  accounts: readonly WorkAccount[],
  group: WorkGroup,
  accountId: string | null = null,
): number {
  return accounts
    .filter((a) => !accountId || a.accountId === accountId)
    .reduce((n, a) => {
      const list = a.lists?.[group];
      return n + (list ? (list.total ?? list.items.length) : 0);
    }, 0);
}

/** Badge count: pull / merge requests waiting for my review, all accounts. */
export const reviewCount = (data: WorkOverview | undefined) =>
  data ? groupTotal(data.accounts, "reviewRequested") : 0;

/**
 * The open repository tab of a project: the active tab first, then any tab with a remote on the
 * same host and full name. `null` when it is not open.
 */
export function findOpenRepo(
  tabs: readonly { id: number; remotes: readonly HostedRemote[] | undefined }[],
  activeId: number | null,
  host: string,
  fullName: string,
): number | null {
  const ordered = [...tabs].sort((a, b) => Number(b.id === activeId) - Number(a.id === activeId));
  const hit = ordered.find((tab) => tab.remotes?.some((r) => sameProject(r, host, fullName)));
  return hit?.id ?? null;
}
