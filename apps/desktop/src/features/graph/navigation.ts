import { ROW_HEIGHT } from "./draw";

export type ScrollAlign = "top" | "center" | "nearest";

/** scrollTop that brings row `index` into a viewport of `height` px, or null if no scroll is needed. */
export function scrollTopFor(
  index: number,
  align: ScrollAlign,
  current: number,
  height: number,
): number | null {
  const rowTop = index * ROW_HEIGHT;
  const rowBottom = rowTop + ROW_HEIGHT;
  switch (align) {
    case "top":
      return rowTop;
    case "center":
      return Math.max(0, rowTop - height / 2 + ROW_HEIGHT / 2);
    case "nearest":
      if (rowTop < current) return rowTop;
      if (rowBottom > current + height) return rowBottom - height;
      return null;
  }
}

/** New selected row for a navigation key, or null if the key is not a navigation key. */
export function nextIndex(
  key: string,
  current: number | null,
  total: number,
  pageRows: number,
): number | null {
  if (total === 0) return null;
  const cur = current ?? -1;
  const clamp = (i: number) => Math.min(total - 1, Math.max(0, i));
  switch (key) {
    case "ArrowDown":
      return clamp(cur + 1);
    case "ArrowUp":
      return clamp(current === null ? 0 : cur - 1);
    case "PageDown":
      return clamp(cur + pageRows);
    case "PageUp":
      return clamp(cur - pageRows);
    case "Home":
      return 0;
    case "End":
      return total - 1;
    default:
      return null;
  }
}

/** Index in `matches` to move to from `current` (wraps around). */
export function cycleMatch(current: number, count: number, backwards: boolean): number {
  if (count === 0) return -1;
  return backwards ? (current - 1 + count) % count : (current + 1) % count;
}
