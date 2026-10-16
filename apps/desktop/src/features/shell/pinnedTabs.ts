/**
 * Pinned repository tabs: shown first, more compact, with a pin instead of the close button,
 * and left alone by "Close others / to the right / all". Kept on this computer by repo id.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { RepoTab } from "@/ipc";

interface PinnedTabsState {
  pinned: number[];
  pin: (id: number) => void;
  unpin: (id: number) => void;
}

export const usePinnedTabs = create<PinnedTabsState>()(
  persist(
    (set) => ({
      pinned: [],
      pin: (id) => set((s) => (s.pinned.includes(id) ? s : { pinned: [...s.pinned, id] })),
      unpin: (id) => set((s) => ({ pinned: s.pinned.filter((p) => p !== id) })),
    }),
    { name: "nanomgg.pinnedTabs", version: 1 },
  ),
);

/** Strip order: pinned tabs first, each group in workspace order. */
export function orderTabs<T extends Pick<RepoTab, "id">>(
  tabs: readonly T[],
  pinned: readonly number[],
): T[] {
  const set = new Set(pinned);
  return [...tabs.filter((t) => set.has(t.id)), ...tabs.filter((t) => !set.has(t.id))];
}

/** Tabs as shown in the strip right now (used by Ctrl+Tab and "close to the right"). */
export function displayedTabs<T extends Pick<RepoTab, "id">>(tabs: readonly T[]): T[] {
  return orderTabs(tabs, usePinnedTabs.getState().pinned);
}

export function isPinned(id: number): boolean {
  return usePinnedTabs.getState().pinned.includes(id);
}

/**
 * Moves `id` before (or after) `overId`. Unchanged when either is unknown, when they are the
 * same tab, or when the move would cross between pinned and unpinned tabs.
 */
export function moveTab(
  order: readonly number[],
  pinned: readonly number[],
  id: number,
  overId: number,
  after: boolean,
): number[] {
  if (id === overId || !order.includes(id) || !order.includes(overId)) return [...order];
  if (pinned.includes(id) !== pinned.includes(overId)) return [...order];
  const rest = order.filter((x) => x !== id);
  const at = rest.indexOf(overId) + (after ? 1 : 0);
  return [...rest.slice(0, at), id, ...rest.slice(at)];
}
