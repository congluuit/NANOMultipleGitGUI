import type { Hunk, LineSelection } from "@/ipc";

/** Selected line indices per hunk index (only `add`/`delete` lines are ever selected). */
export type LineSel = Readonly<Record<number, readonly number[]>>;

export const EMPTY_SEL: LineSel = {};

/** Indices of the changed (`add`/`delete`) lines of a hunk. */
export function hunkChanges(hunk: Hunk): number[] {
  const out: number[] = [];
  hunk.lines.forEach((l, i) => {
    if (l.kind !== "context") out.push(i);
  });
  return out;
}

function withHunk(sel: LineSel, hunk: number, lines: Iterable<number>): LineSel {
  const sorted = [...new Set(lines)].sort((a, b) => a - b);
  const next: Record<number, readonly number[]> = { ...sel };
  if (sorted.length) next[hunk] = sorted;
  else delete next[hunk];
  return next;
}

export function isSelected(sel: LineSel, hunk: number, line: number): boolean {
  return sel[hunk]?.includes(line) ?? false;
}

/** Toggles one changed line; context lines are ignored. */
export function toggleLine(sel: LineSel, hunks: Hunk[], hunk: number, line: number): LineSel {
  if (hunks[hunk]?.lines[line]?.kind === "context" || !hunks[hunk]?.lines[line]) return sel;
  const current = new Set(sel[hunk] ?? []);
  if (current.has(line)) current.delete(line);
  else current.add(line);
  return withHunk(sel, hunk, current);
}

/** Adds every changed line between `from` and `to` (inclusive) of one hunk. */
export function selectRange(
  sel: LineSel,
  hunks: Hunk[],
  hunk: number,
  from: number,
  to: number,
): LineSel {
  const h = hunks[hunk];
  if (!h) return sel;
  const [a, b] = from <= to ? [from, to] : [to, from];
  const add = hunkChanges(h).filter((i) => i >= a && i <= b);
  return withHunk(sel, hunk, [...(sel[hunk] ?? []), ...add]);
}

export function countSelected(sel: LineSel): number {
  return Object.values(sel).reduce((n, lines) => n + lines.length, 0);
}

/** Selection of a whole hunk, in the command's format. */
export function wholeHunk(hunks: Hunk[], hunk: number): LineSelection[] {
  const h = hunks[hunk];
  return h ? [{ hunk, lines: hunkChanges(h) }] : [];
}

export function toSelections(sel: LineSel): LineSelection[] {
  return Object.entries(sel)
    .map(([hunk, lines]) => ({ hunk: Number(hunk), lines: [...lines] }))
    .filter((s) => s.lines.length > 0)
    .sort((a, b) => a.hunk - b.hunk);
}
