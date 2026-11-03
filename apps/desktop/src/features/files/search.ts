import type { SearchOptions } from "@/ipc";

/**
 * Length of the match starting at `start` in a result line, to highlight it. git reports only
 * where the first match starts; the pattern is matched again here (an ERE is close enough to a
 * JavaScript regex for highlighting). Falls back to the query length.
 */
export function matchLength(
  text: string,
  start: number,
  query: string,
  options: SearchOptions,
): number {
  if (!options.regex) return Math.min(query.length, Math.max(0, text.length - start));
  try {
    const re = new RegExp(query, options.caseSensitive ? "y" : "iy");
    re.lastIndex = start;
    const m = re.exec(text);
    if (m && m[0].length > 0) return m[0].length;
  } catch {
    // Not a valid JavaScript regex: highlight the query length.
  }
  return Math.min(query.length, Math.max(0, text.length - start));
}
