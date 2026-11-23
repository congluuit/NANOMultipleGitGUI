/**
 * Pure logic of the bulk operations in "All repositories": filters, grouping by folder or by
 * account, selection and the push order the user arranges.
 */
import type { RepoOverview } from "@/ipc";
import { canFetch } from "./overview";

export type OverviewFilter = "all" | "changes" | "outOfSync";
export const FILTERS: readonly OverviewFilter[] = ["all", "changes", "outOfSync"];

/** Has uncommitted changes or conflicts. */
export const hasChanges = (r: RepoOverview) => r.changes > 0 || r.conflicts > 0;
/** Ahead of or behind its upstream. */
export const isOutOfSync = (r: RepoOverview) => r.ahead > 0 || r.behind > 0;

export function matchesFilter(r: RepoOverview, filter: OverviewFilter): boolean {
  if (filter === "changes") return hasChanges(r);
  if (filter === "outOfSync") return isOutOfSync(r);
  return true;
}

export function filterCounts(rows: readonly RepoOverview[]): Record<OverviewFilter, number> {
  return {
    all: rows.length,
    changes: rows.filter(hasChanges).length,
    outOfSync: rows.filter(isOutOfSync).length,
  };
}

export type GroupMode = "none" | "folder" | "account";
export const GROUP_MODES: readonly GroupMode[] = ["none", "folder", "account"];

/** The account a repository is bound to, as shown on a group header. */
export interface AccountTag {
  id: string;
  label: string;
  color: string | null;
}

export interface RowGroup {
  key: string;
  /** Short name; empty for the single group of the "none" mode. */
  label: string;
  /** Full folder path or account name (tooltip). */
  title: string;
  color: string | null;
  rows: RepoOverview[];
}

/** Key of the group of repositories without an account. */
export const UNBOUND_KEY = "account:none";

/** Folder that contains the repository, with `/` separators and no trailing slash. */
export function parentFolder(path: string): string {
  const norm = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const cut = norm.lastIndexOf("/");
  if (cut < 0) return "";
  // "D:/api" lives in "D:/", "/api" in "/".
  const parent = norm.slice(0, cut);
  return parent === "" || /^[A-Za-z]:$/.test(parent) ? `${parent}/` : parent;
}

/** Last part of a folder path ("D:/Work/libs" gives "libs"); the root stays as it is. */
export function folderLabel(folder: string): string {
  const trimmed = folder.replace(/\/+$/, "");
  const cut = trimmed.lastIndexOf("/");
  return cut < 0 || cut === trimmed.length - 1 ? folder : trimmed.slice(cut + 1);
}

/**
 * Rows in groups. Groups are sorted by name (repositories without an account last); rows keep
 * their order (the tab order) inside a group. Two folders with the same last part are told
 * apart by their full path.
 */
export function groupRows(
  rows: readonly RepoOverview[],
  mode: GroupMode,
  accountOf: (repoId: number) => AccountTag | null,
  unboundLabel = "",
): RowGroup[] {
  if (mode === "none") {
    return rows.length ? [{ key: "all", label: "", title: "", color: null, rows: [...rows] }] : [];
  }
  const groups = new Map<string, RowGroup>();
  for (const row of rows) {
    let key: string;
    let label: string;
    let title: string;
    let color: string | null = null;
    if (mode === "folder") {
      const folder = parentFolder(row.path);
      key = `folder:${folder.toLowerCase()}`;
      label = folderLabel(folder);
      title = folder;
    } else {
      const account = accountOf(row.repoId);
      key = account ? `account:${account.id}` : UNBOUND_KEY;
      label = account?.label ?? unboundLabel;
      title = label;
      color = account?.color ?? null;
    }
    const group = groups.get(key);
    if (group) group.rows.push(row);
    else groups.set(key, { key, label, title, color, rows: [row] });
  }
  const list = [...groups.values()];
  // Same short name twice: show the full paths instead.
  if (mode === "folder") {
    const seen = new Map<string, number>();
    for (const g of list)
      seen.set(g.label.toLowerCase(), (seen.get(g.label.toLowerCase()) ?? 0) + 1);
    for (const g of list) if ((seen.get(g.label.toLowerCase()) ?? 0) > 1) g.label = g.title;
  }
  return list.sort((a, b) => {
    if (a.key === UNBOUND_KEY) return 1;
    if (b.key === UNBOUND_KEY) return -1;
    return a.label.localeCompare(b.label, undefined, { sensitivity: "base" });
  });
}

export type CheckState = "all" | "some" | "none";

/** Whether all, some or none of `ids` are selected (an empty list counts as none). */
export function checkState(ids: readonly number[], selected: ReadonlySet<number>): CheckState {
  const on = ids.filter((id) => selected.has(id)).length;
  if (on === 0) return "none";
  return on === ids.length ? "all" : "some";
}

/** `selected` with `ids` added (`on`) or removed. */
export function setMany(selected: ReadonlySet<number>, ids: readonly number[], on: boolean) {
  const next = new Set(selected);
  for (const id of ids) {
    if (on) next.add(id);
    else next.delete(id);
  }
  return next;
}

/**
 * What a bulk button works on: the selected repositories (in `rows` order) or, with nothing
 * selected, every row given (the visible ones).
 */
export function targetRows(
  all: readonly RepoOverview[],
  visible: readonly RepoOverview[],
  selected: ReadonlySet<number>,
): RepoOverview[] {
  const picked = all.filter((r) => selected.has(r.repoId));
  return picked.length > 0 ? picked : [...visible];
}

/**
 * Has something to push: on a branch with an upstream and ahead of it. A branch that is not on
 * a server yet is left out on purpose: publishing it is a choice made in that repository, not
 * something a bulk push should do to every local branch.
 */
export const canPushRow = (r: RepoOverview) =>
  canFetch(r) && !!r.branch && !!r.upstream && r.ahead > 0;

/**
 * The push order: ids the user already placed keep their place, new ones are added at the end
 * in the order given, ids no longer in `ids` are dropped.
 */
export function syncOrder(order: readonly number[], ids: readonly number[]): number[] {
  const wanted = new Set(ids);
  const kept = order.filter((id) => wanted.has(id));
  const known = new Set(kept);
  return [...kept, ...ids.filter((id) => !known.has(id))];
}

/** `list` with the item at `from` moved to index `to` (clamped). */
export function moveItem<T>(list: readonly T[], from: number, to: number): T[] {
  const next = [...list];
  if (from < 0 || from >= next.length) return next;
  const target = Math.max(0, Math.min(next.length - 1, to));
  const [item] = next.splice(from, 1);
  next.splice(target, 0, item as T);
  return next;
}
