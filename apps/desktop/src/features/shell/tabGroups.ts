/**
 * "Group tabs by account": puts the tabs of the same account next to each other, so the
 * colored lines under them form one block per account.
 */
import i18n from "i18next";
import { cachedRepoAccount } from "@/features/accounts/switcher/hooks";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { usePinnedTabs } from "./pinnedTabs";

/**
 * New tab order: pinned tabs stay where they are; the others are grouped by account in the
 * order each account first appears, tabs without an account last. Order inside a group is
 * kept.
 */
export function groupByAccount(
  order: readonly number[],
  pinned: readonly number[],
  accountOf: (id: number) => string | null,
): number[] {
  const groups = new Map<string, number[]>();
  const none: number[] = [];
  for (const id of order) {
    if (pinned.includes(id)) continue;
    const account = accountOf(id);
    if (account === null) none.push(id);
    else groups.set(account, [...(groups.get(account) ?? []), id]);
  }
  const grouped = [...[...groups.values()].flat(), ...none];
  let next = 0;
  return order.map((id) => (pinned.includes(id) ? id : grouped[next++]!));
}

/** Whether grouping would change anything. */
export function canGroupByAccount(
  order: readonly number[],
  pinned: readonly number[],
  accountOf: (id: number) => string | null,
): boolean {
  return groupByAccount(order, pinned, accountOf).some((id, i) => id !== order[i]);
}

export const accountOfTab = (id: number) => cachedRepoAccount(id).account?.id ?? null;

/** Regroups the open tabs and saves the order. */
export async function groupTabsByAccount() {
  const { tabs, reorder } = useWorkspace.getState();
  const order = tabs.map((t) => t.id);
  const next = groupByAccount(order, usePinnedTabs.getState().pinned, accountOfTab);
  if (next.every((id, i) => id === order[i])) {
    useUi.getState().showNotice({ kind: "info", message: i18n.t("tabs.alreadyGrouped") });
    return;
  }
  await reorder(next);
}
