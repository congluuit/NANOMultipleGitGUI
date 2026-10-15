/**
 * Conflict markers in a file git left after a failed merge:
 *
 *     <<<<<<< ours-label
 *     our lines
 *     ||||||| base-label        (only with merge.conflictStyle = diff3 / zdiff3)
 *     base lines
 *     =======
 *     their lines
 *     >>>>>>> theirs-label
 */

export interface ConflictBlock {
  /** 0-based index of the `<<<<<<<` line. */
  start: number;
  /** 0-based index of the `>>>>>>>` line. */
  end: number;
  /** 0-based index of the `=======` line. */
  mid: number;
  /** 0-based index of the `|||||||` line, if any. */
  baseAt: number | null;
  ours: string[];
  /** `null` without a base section (default conflict style). */
  base: string[] | null;
  theirs: string[];
  oursLabel: string;
  baseLabel: string | null;
  theirsLabel: string;
}

/** How to resolve one block. */
export type BlockChoice = "ours" | "theirs" | "oursThenTheirs" | "theirsThenOurs" | "base";

/** Lines of a text without their line endings, and the dominant line ending. */
export function splitLines(text: string): { lines: string[]; eol: "\n" | "\r\n" } {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  const lf = (text.match(/\n/g) ?? []).length;
  return {
    lines: text.split("\n").map((l) => (l.endsWith("\r") ? l.slice(0, -1) : l)),
    eol: crlf > 0 && crlf * 2 >= lf ? "\r\n" : "\n",
  };
}

/** Label after a marker (`<<<<<<< HEAD` → `HEAD`), or `null` when the line is not that marker. */
function marker(line: string, c: string): string | null {
  if (line.length < 7) return null;
  for (let i = 0; i < 7; i++) if (line[i] !== c) return null;
  if (line.length === 7) return "";
  if (c === "=" || line[7] !== " ") return null;
  return line.slice(8);
}

/** Complete conflict blocks of a text, in order. Unfinished blocks are ignored. */
export function parseConflicts(text: string): ConflictBlock[] {
  return parseLines(splitLines(text).lines);
}

export function parseLines(lines: string[]): ConflictBlock[] {
  const blocks: ConflictBlock[] = [];
  let cur: ConflictBlock | null = null;
  let section: "ours" | "base" | "theirs" = "ours";
  lines.forEach((line, i) => {
    if (!cur) {
      const label = marker(line, "<");
      if (label !== null) {
        cur = {
          start: i,
          end: i,
          mid: i,
          baseAt: null,
          ours: [],
          base: null,
          theirs: [],
          oursLabel: label,
          baseLabel: null,
          theirsLabel: "",
        };
        section = "ours";
      }
      return;
    }
    const b: ConflictBlock = cur;
    if (section === "ours" && marker(line, "|") !== null) {
      b.base = [];
      b.baseAt = i;
      b.baseLabel = marker(line, "|");
      section = "base";
    } else if (section !== "theirs" && marker(line, "=") !== null) {
      b.mid = i;
      section = "theirs";
    } else if (section === "theirs" && marker(line, ">") !== null) {
      b.end = i;
      b.theirsLabel = marker(line, ">") ?? "";
      blocks.push(b);
      cur = null;
    } else if (section === "ours" && marker(line, "<") !== null) {
      // A new block started before this one ended: drop the broken one.
      cur = { ...b, start: i, mid: i, ours: [], oursLabel: marker(line, "<") ?? "" };
    } else {
      (section === "base" ? b.base! : section === "ours" ? b.ours : b.theirs).push(line);
    }
  });
  return blocks;
}

/** Lines that replace a block for a choice. */
export function replacement(block: ConflictBlock, choice: BlockChoice): string[] {
  switch (choice) {
    case "ours":
      return block.ours;
    case "theirs":
      return block.theirs;
    case "oursThenTheirs":
      return [...block.ours, ...block.theirs];
    case "theirsThenOurs":
      return [...block.theirs, ...block.ours];
    case "base":
      return block.base ?? [];
  }
}

/** The text with block `index` resolved (keeping the text's line endings). */
export function resolveBlock(text: string, index: number, choice: BlockChoice): string {
  const { lines, eol } = splitLines(text);
  const block = parseLines(lines)[index];
  if (!block) return text;
  const next = [
    ...lines.slice(0, block.start),
    ...replacement(block, choice),
    ...lines.slice(block.end + 1),
  ];
  return next.join(eol);
}

/** Every block resolved the same way. */
export function resolveAll(text: string, choice: BlockChoice): string {
  let out = text;
  while (parseConflicts(out).length > 0) out = resolveBlock(out, 0, choice);
  return out;
}

/** Index of the block containing `line` (0-based), else of the next one after it, else -1. */
export function blockAt(blocks: ConflictBlock[], line: number): number {
  const inside = blocks.findIndex((b) => line >= b.start && line <= b.end);
  if (inside >= 0) return inside;
  return blocks.findIndex((b) => b.start > line);
}

/** Next / previous block from the current line, wrapping around. */
export function stepBlock(blocks: ConflictBlock[], line: number, dir: 1 | -1): number {
  if (blocks.length === 0) return -1;
  if (dir === 1) {
    const i = blocks.findIndex((b) => b.start > line);
    return i >= 0 ? i : 0;
  }
  for (let i = blocks.length - 1; i >= 0; i--) if (blocks[i]!.end < line) return i;
  return blocks.length - 1;
}

/** A Monaco-style edit (1-based lines and columns) replacing a block with `lines`. */
export interface LineEdit {
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
  lines: string[];
}

/**
 * Edit that replaces block lines `start..end` (0-based) of a document with `lineCount` lines;
 * `lineLength(n)` gives the length of 1-based line `n`. An empty replacement removes the
 * lines together with their line break.
 */
export function blockEdit(
  block: Pick<ConflictBlock, "start" | "end">,
  lines: string[],
  lineCount: number,
  lineLength: (line: number) => number,
): LineEdit {
  const first = block.start + 1;
  const last = block.end + 1;
  if (lines.length > 0) {
    return {
      startLine: first,
      startColumn: 1,
      endLine: last,
      endColumn: lineLength(last) + 1,
      lines,
    };
  }
  if (last < lineCount) {
    return { startLine: first, startColumn: 1, endLine: last + 1, endColumn: 1, lines: [] };
  }
  if (first > 1) {
    return {
      startLine: first - 1,
      startColumn: lineLength(first - 1) + 1,
      endLine: last,
      endColumn: lineLength(last) + 1,
      lines: [],
    };
  }
  return { startLine: 1, startColumn: 1, endLine: last, endColumn: lineLength(last) + 1, lines };
}

/** First index where `needle` appears as consecutive lines of `haystack`, else -1. */
export function locate(haystack: string[], needle: string[]): number {
  if (needle.length === 0) return -1;
  outer: for (let i = 0; i + needle.length <= haystack.length; i++) {
    for (let j = 0; j < needle.length; j++) if (haystack[i + j] !== needle[j]) continue outer;
    return i;
  }
  return -1;
}
