import type { BisectState, ReflogEntry, SparseState } from "@/ipc";

/** Entries whose action, message, subject or commit id contain `query`. */
export function filterReflog(entries: readonly ReflogEntry[], query: string): ReflogEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...entries];
  return entries.filter((e) =>
    [e.action, e.message, e.subject, e.oid].some((text) => text.toLowerCase().includes(q)),
  );
}

/** Share of `value` in `max` as a bar width, never invisible for a non-zero value. */
export function barPercent(value: number, max: number): number {
  if (value <= 0 || max <= 0) return 0;
  return Math.max(2, Math.round((value / max) * 100));
}

/** Folders typed one per line, trimmed, without empty lines or duplicates. */
export function parseFolderLines(text: string): string[] {
  const out: string[] = [];
  for (const line of text.split(/\r?\n/)) {
    const folder = line
      .trim()
      .replace(/\\/g, "/")
      .replace(/^\/+|\/+$/g, "");
    if (folder && !out.includes(folder)) out.push(folder);
  }
  return out;
}

/** Kept folders that are not top-level ones (shown in the text box). */
export function nestedFolders(state: Pick<SparseState, "folders" | "available">): string[] {
  return state.folders.filter((f) => !state.available.includes(f));
}

/**
 * Rounds of testing a bisect still needs, at most: each answer halves the candidates.
 * `remaining` counts the commits that can be the first bad one.
 */
export function bisectStepsLeft(state: Pick<BisectState, "remaining" | "firstBad">): number {
  if (state.firstBad || state.remaining <= 1) return 0;
  return Math.ceil(Math.log2(state.remaining));
}
