/**
 * Line diff for the editor's change markers (the "dirty diff" gutter of VS Code). Runs on every
 * edit, so it is a plain Myers diff over interned lines with the common head and tail trimmed
 * first; past `maxCost` edits the differing middle is reported as one change.
 */

/** One changed block. Line numbers are 0-based; a count of 0 marks a pure insertion/deletion. */
export interface LineChange {
  originalStart: number;
  originalCount: number;
  modifiedStart: number;
  modifiedCount: number;
}

export type ChangeKind = "added" | "modified" | "deleted";

export function changeKind(c: LineChange): ChangeKind {
  if (c.originalCount === 0) return "added";
  if (c.modifiedCount === 0) return "deleted";
  return "modified";
}

/** Splits text into lines the way Monaco does (`\r\n`, `\r` or `\n`; a leading BOM dropped). */
export function splitLines(text: string): string[] {
  return text.replace(/^\uFEFF/, "").split(/\r\n|\r|\n/);
}

export function diffLines(
  a: readonly string[],
  b: readonly string[],
  maxCost = 2000,
): LineChange[] {
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  ) {
    tail++;
  }
  const n = a.length - head - tail;
  const m = b.length - head - tail;
  if (n === 0 && m === 0) return [];
  if (n === 0 || m === 0) {
    return [{ originalStart: head, originalCount: n, modifiedStart: head, modifiedCount: m }];
  }

  // Intern the lines so the inner loop compares numbers.
  const ids = new Map<string, number>();
  const intern = (lines: readonly string[], count: number) => {
    const out = new Int32Array(count);
    for (let i = 0; i < count; i++) {
      const line = lines[head + i] ?? "";
      let id = ids.get(line);
      if (id === undefined) {
        id = ids.size;
        ids.set(line, id);
      }
      out[i] = id;
    }
    return out;
  };
  const x = intern(a, n);
  const y = intern(b, m);

  const edits = myers(x, y, maxCost);
  if (!edits) {
    return [{ originalStart: head, originalCount: n, modifiedStart: head, modifiedCount: m }];
  }
  return group(edits.removed, edits.added, head);
}

/** Marks which lines of `a` are removed and which of `b` are added; `null` past `maxCost`. */
function myers(
  a: Int32Array,
  b: Int32Array,
  maxCost: number,
): { removed: Uint8Array; added: Uint8Array } | null {
  const n = a.length;
  const m = b.length;
  const max = n + m;
  const offset = max + 1;
  const v = new Int32Array(2 * max + 3);
  // trace[d] holds v[-d-1 ..= d+1] as it was before step d.
  const trace: Int32Array[] = [];
  let found = -1;
  for (let d = 0; d <= Math.min(max, maxCost); d++) {
    trace.push(v.slice(offset - d - 1, offset + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let px: number;
      if (k === -d || (k !== d && v[offset + k - 1]! < v[offset + k + 1]!)) px = v[offset + k + 1]!;
      else px = v[offset + k - 1]! + 1;
      let py = px - k;
      while (px < n && py < m && a[px] === b[py]) {
        px++;
        py++;
      }
      v[offset + k] = px;
      if (px >= n && py >= m) {
        found = d;
        break;
      }
    }
    if (found >= 0) break;
  }
  if (found < 0) return null;

  const removed = new Uint8Array(n);
  const added = new Uint8Array(m);
  let px = n;
  let py = m;
  for (let d = found; d > 0; d--) {
    const snap = trace[d]!;
    const at = (k: number) => snap[k + d + 1]!;
    const k = px - py;
    const prevK = k === -d || (k !== d && at(k - 1) < at(k + 1)) ? k + 1 : k - 1;
    const prevX = at(prevK);
    const prevY = prevX - prevK;
    while (px > prevX && py > prevY) {
      px--;
      py--;
    }
    if (px === prevX) added[prevY] = 1;
    else removed[prevX] = 1;
    px = prevX;
    py = prevY;
  }
  return { removed, added };
}

function group(removed: Uint8Array, added: Uint8Array, head: number): LineChange[] {
  const out: LineChange[] = [];
  let i = 0;
  let j = 0;
  while (i < removed.length || j < added.length) {
    if (i < removed.length && j < added.length && !removed[i] && !added[j]) {
      i++;
      j++;
      continue;
    }
    const si = i;
    const sj = j;
    while ((i < removed.length && removed[i]) || (j < added.length && added[j])) {
      while (i < removed.length && removed[i]) i++;
      while (j < added.length && added[j]) j++;
    }
    out.push({
      originalStart: head + si,
      originalCount: i - si,
      modifiedStart: head + sj,
      modifiedCount: j - sj,
    });
  }
  return out;
}

/** Applies the original lines of `change` back onto `modified` (what "Revert change" does). */
export function revertLines(
  original: readonly string[],
  modified: readonly string[],
  change: LineChange,
): string[] {
  return [
    ...modified.slice(0, change.modifiedStart),
    ...original.slice(change.originalStart, change.originalStart + change.originalCount),
    ...modified.slice(change.modifiedStart + change.modifiedCount),
  ];
}

/** Applies the modified lines of `change` onto `original` (what "Stage this change" does). */
export function applyChange(
  original: readonly string[],
  modified: readonly string[],
  change: LineChange,
): string[] {
  return [
    ...original.slice(0, change.originalStart),
    ...modified.slice(change.modifiedStart, change.modifiedStart + change.modifiedCount),
    ...original.slice(change.originalStart + change.originalCount),
  ];
}

/**
 * 0-based line of the original that modified `line` comes from, `null` when the line is part
 * of a change (edited or added).
 */
export function originalLine(changes: readonly LineChange[], line: number): number | null {
  let delta = 0;
  for (const c of changes) {
    if (line < c.modifiedStart) break;
    if (line < c.modifiedStart + c.modifiedCount) return null;
    delta += c.originalCount - c.modifiedCount;
  }
  return line + delta;
}

/** A text edit in Monaco's 1-based coordinates. */
export interface LineEdit {
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
  text: string;
}

/**
 * The edit that puts the original lines of `change` back into a document whose lines are
 * `modified`, so it can go through the editor (undoable) instead of replacing the whole text.
 */
export function revertEdit(
  original: readonly string[],
  modified: readonly string[],
  change: LineChange,
  eol: string,
): LineEdit {
  const lines = original.slice(change.originalStart, change.originalStart + change.originalCount);
  const start = change.modifiedStart;
  const end = start + change.modifiedCount;
  const last = modified.length;
  const endOf = (line: number) => (modified[line - 1]?.length ?? 0) + 1;
  // A line follows the block: replace whole lines, each ending with its line break.
  if (end < last) {
    return {
      startLine: start + 1,
      startColumn: 1,
      endLine: end + 1,
      endColumn: 1,
      text: lines.map((l) => l + eol).join(""),
    };
  }
  // The block reaches the end: take the line break before it instead.
  if (start > 0) {
    return {
      startLine: start,
      startColumn: endOf(start),
      endLine: last,
      endColumn: endOf(last),
      text: lines.map((l) => eol + l).join(""),
    };
  }
  return {
    startLine: 1,
    startColumn: 1,
    endLine: last,
    endColumn: endOf(last),
    text: lines.join(eol),
  };
}

/** Index of the change touching 0-based `line`, a deletion counting for the line above it. */
export function changeAt(changes: readonly LineChange[], line: number): number {
  return changes.findIndex((c) =>
    c.modifiedCount === 0
      ? line === Math.max(0, c.modifiedStart - 1)
      : line >= c.modifiedStart && line < c.modifiedStart + c.modifiedCount,
  );
}
