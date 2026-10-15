import { useLayoutEffect, type ReactNode, type RefObject } from "react";
import type { LucideIcon } from "lucide-react";
import { create } from "zustand";

/** What the "⋯" menu needs to stand in for a toolbar button that does not fit. */
export interface OverflowItem {
  icon: LucideIcon;
  label: string;
  tooltip?: string;
  disabled?: boolean;
  busy?: boolean;
  /** Runs the button's current click handler. */
  run: () => void;
  /** Label + current items of the button's dropdown (Pull ▾ / Push ▾), if any. */
  menuLabel?: string;
  renderMenu?: () => ReactNode;
}

interface OverflowState {
  items: Record<string, OverflowItem>;
  /** Ids of the buttons moved into the "⋯" menu, in toolbar order. */
  hidden: string[];
  register: (id: string, item: OverflowItem) => void;
  unregister: (id: string) => void;
  setHidden: (hidden: string[]) => void;
}

export const useToolbarOverflow = create<OverflowState>()((set, get) => ({
  items: {},
  hidden: [],
  register: (id, item) => set({ items: { ...get().items, [id]: item } }),
  unregister: (id) => {
    const { [id]: _gone, ...items } = get().items;
    set({ items });
  },
  setHidden: (hidden) => {
    if (hidden.join("\n") !== get().hidden.join("\n")) set({ hidden });
  },
}));

/** Width kept for the "⋯" button whenever something overflows. */
export const MORE_WIDTH = 40;
/** Assumed width of a button never seen yet (hidden from the first layout). */
const DEFAULT_ITEM_WIDTH = 64;
/** `data-toolbar-priority`: lower values move into the menu first; `pin` never does. */
export const DEFAULT_PRIORITY = 10;

interface Measured {
  id: string;
  width: number;
  priority: number;
  index: number;
}

/**
 * Which buttons to hide so the rest fits in `available` px. Lowest priority first and, among
 * equals, the rightmost first. `fixed` is everything else on the row (separators, gaps…).
 */
export function chooseHidden(items: Measured[], fixed: number, available: number): string[] {
  const total = items.reduce((n, i) => n + i.width, fixed);
  if (available <= 0 || total <= available) return [];
  let need = total + MORE_WIDTH - available;
  const order = [...items]
    .filter((i) => Number.isFinite(i.priority))
    .sort((a, b) => a.priority - b.priority || b.index - a.index);
  const hidden = new Set<string>();
  for (const item of order) {
    if (need <= 0) break;
    hidden.add(item.id);
    need -= item.width;
  }
  return items.filter((i) => hidden.has(i.id)).map((i) => i.id);
}

const widths = new Map<string, number>();

/**
 * Measures the toolbar row (`ResizeObserver`) and moves the buttons that do not fit into the
 * "⋯" menu. Buttons are direct children marked `data-toolbar-item`; a hidden button keeps the
 * width it last had while visible. Children marked `data-toolbar-flex` (spacers) count as 0.
 */
export function useToolbarOverflowLayout(ref: RefObject<HTMLElement | null>) {
  useLayoutEffect(() => {
    const row = ref.current;
    if (!row) return;
    const compute = () => {
      const style = getComputedStyle(row);
      const gap = parseFloat(style.columnGap) || 0;
      const pad = (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0);
      const available = row.clientWidth - pad;
      const items: Measured[] = [];
      let fixed = 0;
      const children = Array.from(row.children) as HTMLElement[];
      let shown = 0;
      children.forEach((el, index) => {
        const id = el.dataset.toolbarItem;
        if (id) {
          if (!el.hidden && el.offsetWidth > 0) widths.set(id, el.offsetWidth);
          const p = el.dataset.toolbarPriority;
          items.push({
            id,
            width: (widths.get(id) ?? DEFAULT_ITEM_WIDTH) + gap,
            priority: p === "pin" ? Number.POSITIVE_INFINITY : p ? Number(p) : DEFAULT_PRIORITY,
            index,
          });
        } else if (el.dataset.toolbarMore === undefined && el.dataset.toolbarFlex === undefined) {
          if (el.offsetWidth > 0) {
            const s = getComputedStyle(el);
            const margin = (parseFloat(s.marginLeft) || 0) + (parseFloat(s.marginRight) || 0);
            fixed += el.offsetWidth + margin + gap;
          }
        }
        if (el.offsetWidth > 0) shown++;
      });
      // Nothing laid out (jsdom, not attached yet): leave everything visible.
      if (shown === 0) return useToolbarOverflow.getState().setHidden([]);
      useToolbarOverflow.getState().setHidden(chooseHidden(items, fixed, available));
    };
    compute();
    const ro = new ResizeObserver(compute);
    ro.observe(row);
    // A button changing width (language, busy label) re-runs the layout too.
    const mo = new MutationObserver(() => {
      ro.disconnect();
      ro.observe(row);
      for (const el of Array.from(row.children)) ro.observe(el);
      compute();
    });
    for (const el of Array.from(row.children)) ro.observe(el);
    mo.observe(row, { childList: true });
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, [ref]);
}
