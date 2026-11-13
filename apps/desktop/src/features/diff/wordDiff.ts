/**
 * Changes inside a line: when a removed line is followed by the line that replaces it, the
 * words that differ are highlighted, so a renamed variable or a changed number stands out
 * instead of two whole lines in red and green.
 */
import type { DiffLine } from "@/ipc";

/** `[start, end)` character offsets of a changed part of a line. */
export type Range = readonly [number, number];

/** Lines longer than this are compared by their common prefix / suffix only. */
const MAX_TOKENS = 400;
/** Above this share of changed characters the two lines are just different lines. */
const MAX_CHANGED = 0.7;
/** Blocks of more lines than this are not paired (a rewrite, not an edit). */
const MAX_BLOCK = 200;

/** Words, runs of spaces and single punctuation characters. */
export function tokenize(text: string): string[] {
  return text.match(/[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]/gu) ?? [];
}

/** Which tokens of `a` and `b` are part of their longest common subsequence. */
function commonTokens(a: string[], b: string[]): { a: boolean[]; b: boolean[] } {
  const keptA = new Array<boolean>(a.length).fill(false);
  const keptB = new Array<boolean>(b.length).fill(false);
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) {
    keptA[start] = keptB[start] = true;
    start += 1;
  }
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA -= 1;
    endB -= 1;
    keptA[endA] = keptB[endB] = true;
  }
  const n = endA - start;
  const m = endB - start;
  if (n === 0 || m === 0 || n > MAX_TOKENS || m > MAX_TOKENS) return { a: keptA, b: keptB };
  // Classic LCS table over the middle part.
  const width = m + 1;
  const table = new Uint16Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i * width + j] =
        a[start + i] === b[start + j]
          ? table[(i + 1) * width + j + 1]! + 1
          : Math.max(table[(i + 1) * width + j]!, table[i * width + j + 1]!);
    }
  }
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[start + i] === b[start + j]) {
      keptA[start + i] = keptB[start + j] = true;
      i += 1;
      j += 1;
    } else if (table[(i + 1) * width + j]! >= table[i * width + j + 1]!) {
      i += 1;
    } else {
      j += 1;
    }
  }
  return { a: keptA, b: keptB };
}

/** Character ranges of the tokens not kept, neighbours merged. */
function changedRanges(tokens: string[], kept: boolean[]): Range[] {
  const out: [number, number][] = [];
  let offset = 0;
  tokens.forEach((token, i) => {
    const end = offset + token.length;
    if (!kept[i]) {
      const last = out[out.length - 1];
      if (last && last[1] === offset) last[1] = end;
      else out.push([offset, end]);
    }
    offset = end;
  });
  return out;
}

const length = (ranges: Range[]) => ranges.reduce((sum, [s, e]) => sum + (e - s), 0);

/**
 * Changed parts of an old line and the line that replaced it. `null` when they have too
 * little in common (highlighting almost everything says nothing) or nothing differs.
 */
export function inlineChanges(
  oldText: string,
  newText: string,
): { old: Range[]; new: Range[] } | null {
  if (oldText === newText) return null;
  const a = tokenize(oldText);
  const b = tokenize(newText);
  const kept = commonTokens(a, b);
  const oldRanges = changedRanges(a, kept.a);
  const newRanges = changedRanges(b, kept.b);
  const total = oldText.trim().length + newText.trim().length;
  if (total === 0) return null;
  if ((length(oldRanges) + length(newRanges)) / total > MAX_CHANGED) return null;
  return { old: oldRanges, new: newRanges };
}

/**
 * Highlight ranges for every line of a hunk (`null`: none). A run of removed lines directly
 * followed by a run of the same number of added lines is read as "these lines were edited"
 * and paired in order.
 */
export function hunkHighlights(lines: readonly DiffLine[]): (Range[] | null)[] {
  const out: (Range[] | null)[] = lines.map(() => null);
  let i = 0;
  while (i < lines.length) {
    if (lines[i]!.kind !== "delete") {
      i += 1;
      continue;
    }
    const delStart = i;
    while (i < lines.length && lines[i]!.kind === "delete") i += 1;
    const addStart = i;
    while (i < lines.length && lines[i]!.kind === "add") i += 1;
    const dels = addStart - delStart;
    const adds = i - addStart;
    if (dels !== adds || dels > MAX_BLOCK) continue;
    for (let k = 0; k < dels; k += 1) {
      const change = inlineChanges(lines[delStart + k]!.text, lines[addStart + k]!.text);
      if (!change) continue;
      if (change.old.length) out[delStart + k] = change.old;
      if (change.new.length) out[addStart + k] = change.new;
    }
  }
  return out;
}

/** Splits `text` into plain and changed pieces, in order. */
export function splitByRanges(
  text: string,
  ranges: readonly Range[],
): { text: string; changed: boolean }[] {
  const out: { text: string; changed: boolean }[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    if (start > at) out.push({ text: text.slice(at, start), changed: false });
    out.push({ text: text.slice(start, end), changed: true });
    at = end;
  }
  if (at < text.length) out.push({ text: text.slice(at), changed: false });
  return out;
}
