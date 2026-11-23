/**
 * Table diff for CSV and TSV files: an RFC 4180 parser, header detection, column matching,
 * row pairing (by a key column or by position) and cell level changes.
 *
 * Ideas compared with ioz-labs/gitfleet (MIT), `crates/gitfleet-ui/src/views/csv_diff.rs`:
 * delimiter sniffing, prefix/suffix trimming before aligning identical rows, and a cap on how
 * much of the table is rendered.
 */

const TABLE_EXT = /\.(csv|tsv)$/i;

/** Whether the diff view offers the table mode for this path. */
export function isTablePath(path: string): boolean {
  return TABLE_EXT.test(path);
}

/** Above this size (characters, on either side) the table is not built and the text diff is used. */
export const TABLE_MAX_CHARS = 2 * 1024 * 1024;
/** Above this many records on either side, same. */
export const TABLE_MAX_ROWS = 50_000;
/** Rows rendered at first, and how many more each "Show more" adds. */
export const ROW_PAGE = 300;

export type Delimiter = "," | ";" | "\t" | "|";

/** Tab for `.tsv`; otherwise the separator that appears most in the first line, comma by default. */
export function detectDelimiter(path: string, sample: string): Delimiter {
  if (/\.tsv$/i.test(path)) return "\t";
  const text = sample.charCodeAt(0) === 0xfeff ? sample.slice(1) : sample;
  const first = text.split(/\r?\n/).find((l) => l.trim() !== "") ?? "";
  const counts: Record<Delimiter, number> = { ",": 0, ";": 0, "\t": 0, "|": 0 };
  let quoted = false;
  for (const ch of first) {
    if (ch === '"') quoted = !quoted;
    else if (!quoted && ch in counts) counts[ch as Delimiter] += 1;
  }
  if (counts[","] > 0) return ",";
  const best = (Object.keys(counts) as Delimiter[]).reduce((a, b) =>
    counts[b] > counts[a] ? b : a,
  );
  return counts[best] > 0 ? best : ",";
}

/**
 * Parses delimited text into records. Handles quoted fields with doubled quotes, delimiters
 * and line breaks inside quotes, LF, CRLF and lone CR line endings, a leading BOM, and an
 * unterminated quote (the rest of the text becomes the field). Blank lines at the end are dropped.
 */
export function parseCsv(text: string, delimiter: string): string[][] {
  const n = text.length;
  let i = text.charCodeAt(0) === 0xfeff ? 1 : 0;
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  let inQuotes = false;
  while (i < n) {
    if (inQuotes) {
      const q = text.indexOf('"', i);
      if (q < 0) {
        field += text.slice(i);
        break;
      }
      field += text.slice(i, q);
      if (text[q + 1] === '"') {
        field += '"';
        i = q + 2;
      } else {
        inQuotes = false;
        i = q + 1;
      }
      continue;
    }
    const c = text[i];
    if (c === '"' && field === "" && !quoted) {
      inQuotes = true;
      quoted = true;
      i += 1;
    } else if (c === delimiter) {
      row.push(field);
      field = "";
      quoted = false;
      i += 1;
    } else if (c === "\n" || c === "\r") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
      quoted = false;
      i += c === "\r" && text[i + 1] === "\n" ? 2 : 1;
    } else {
      // A run of plain characters; a quote here (not at the start of a field) is literal.
      let j = i + 1;
      while (j < n) {
        const d = text[j];
        if (d === delimiter || d === "\n" || d === "\r") break;
        j += 1;
      }
      field += text.slice(i, j);
      i = j;
    }
  }
  if (inQuotes || quoted || field !== "" || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  for (let last = rows.at(-1); last && last.length === 1 && last[0] === ""; last = rows.at(-1)) {
    rows.pop();
  }
  return rows;
}

const NUMBER = /^[-+]?(\d{1,3}(,\d{3})+|\d+)?(\.\d+)?([eE][-+]?\d+)?%?$/;
const DATE = /^\d{1,4}[-/.]\d{1,2}[-/.]\d{1,4}([ T]\d{1,2}:\d{2}(:\d{2})?)?/;

/** A value that reads as data rather than a column title: a number, a date, true or false. */
export function isValueLike(cell: string): boolean {
  const s = cell.trim();
  if (s === "") return false;
  if (/^(true|false)$/i.test(s)) return true;
  return (NUMBER.test(s) && /\d/.test(s)) || DATE.test(s);
}

/**
 * Whether the first record looks like column titles: every cell filled in, all different, none
 * a number or date, and on balance the columns below hold numbers or dates rather than repeating
 * the title (in the spirit of Python's `csv.Sniffer.has_header`).
 */
export function detectHeader(rows: string[][]): boolean {
  const first = rows[0];
  if (!first) return false;
  const cells = first.map((c) => c.trim());
  if (cells.some((c) => c === "" || isValueLike(c))) return false;
  if (new Set(cells).size !== cells.length) return false;
  const body = rows.slice(1, 201);
  let votes = 0;
  cells.forEach((title, col) => {
    const values = body.map((r) => (r[col] ?? "").trim()).filter((v) => v !== "");
    if (values.length === 0) return;
    if (values.every(isValueLike)) votes += 1;
    else if (values.includes(title)) votes -= 1;
  });
  return votes >= 0;
}

export type ColumnStatus = "same" | "renamed" | "added" | "removed";

export interface TableColumn {
  /** Header text on the new side (old side for a removed column); `null` without a header row. */
  name: string | null;
  /** Header text on the old side when the column was renamed. */
  oldName: string | null;
  oldIndex: number | null;
  newIndex: number | null;
  status: ColumnStatus;
}

const width = (rows: string[][]) => rows.reduce((w, r) => Math.max(w, r.length), 0);

function padHeader(header: string[], w: number): string[] {
  return header.length >= w ? header : [...header, ...Array<string>(w - header.length).fill("")];
}

/**
 * Lines up the columns of both sides. With a header row columns match by title (a repeated
 * title by its occurrence), so moved columns stay paired; an unmatched title on both sides at
 * the same position is a rename. Without a header they match by position. Removed columns are
 * placed after the column they followed on the old side.
 */
export function matchColumns(
  oldHeader: string[] | null,
  newHeader: string[] | null,
  oldWidth: number,
  newWidth: number,
): TableColumn[] {
  if (!oldHeader || !newHeader) {
    const cols: TableColumn[] = [];
    for (let i = 0; i < Math.max(oldWidth, newWidth); i++) {
      const inOld = i < oldWidth;
      const inNew = i < newWidth;
      cols.push({
        name: newHeader?.[i] ?? oldHeader?.[i] ?? null,
        oldName: null,
        oldIndex: inOld ? i : null,
        newIndex: inNew ? i : null,
        status: inOld && inNew ? "same" : inNew ? "added" : "removed",
      });
    }
    return cols;
  }
  const oldH = padHeader(oldHeader, oldWidth);
  const newH = padHeader(newHeader, newWidth);
  const keyed = (h: string[]) => {
    const seen = new Map<string, number>();
    return h.map((name) => {
      const k = seen.get(name) ?? 0;
      seen.set(name, k + 1);
      return `${name}\u0000${k}`;
    });
  };
  const oldKeys = keyed(oldH);
  const oldByKey = new Map(oldKeys.map((k, i) => [k, i]));
  const oldUsed = new Array<boolean>(oldH.length).fill(false);
  const cols: TableColumn[] = keyed(newH).map((k, j) => {
    const i = oldByKey.get(k);
    if (i !== undefined) oldUsed[i] = true;
    return {
      name: newH[j] ?? "",
      oldName: null,
      oldIndex: i ?? null,
      newIndex: j,
      status: i !== undefined ? "same" : "added",
    };
  });
  for (const c of cols) {
    const j = c.newIndex!;
    if (c.status === "added" && j < oldH.length && !oldUsed[j]) {
      oldUsed[j] = true;
      c.oldIndex = j;
      c.status = "renamed";
      c.oldName = oldH[j] ?? "";
    }
  }
  oldH.forEach((name, i) => {
    if (oldUsed[i]) return;
    let at = -1;
    let best = -1;
    cols.forEach((c, pos) => {
      if (c.oldIndex !== null && c.oldIndex < i && c.oldIndex > best) {
        best = c.oldIndex;
        at = pos;
      }
    });
    cols.splice(at + 1, 0, {
      name,
      oldName: null,
      oldIndex: i,
      newIndex: null,
      status: "removed",
    });
  });
  return cols;
}

/** Whether every record has a filled in, distinct value in this column. */
export function isUniqueColumn(rows: string[][], index: number): boolean {
  const seen = new Set<string>();
  for (const r of rows) {
    const v = (r[index] ?? "").trim();
    if (v === "" || seen.has(v)) return false;
    seen.add(v);
  }
  return true;
}

/** A pairing of an old record index with a new one; `null` on the side where the row is missing. */
export type RowPair = [oldIndex: number | null, newIndex: number | null];

/**
 * Pairs rows by the value in a key column. A repeated value pairs by occurrence. Rows come out
 * in new file order, a removed row right after the row it followed on the old side.
 */
export function pairByKey(
  oldRows: string[][],
  newRows: string[][],
  oldCol: number,
  newCol: number,
): RowPair[] {
  const oldByKey = new Map<string, number[]>();
  oldRows.forEach((r, i) => {
    const k = (r[oldCol] ?? "").trim();
    const list = oldByKey.get(k);
    if (list) list.push(i);
    else oldByKey.set(k, [i]);
  });
  const taken = new Map<string, number>();
  const oldUsed = new Array<boolean>(oldRows.length).fill(false);
  const newToOld = newRows.map((r) => {
    const k = (r[newCol] ?? "").trim();
    const nth = taken.get(k) ?? 0;
    taken.set(k, nth + 1);
    const i = oldByKey.get(k)?.[nth];
    if (i === undefined) return null;
    oldUsed[i] = true;
    return i;
  });
  const out: RowPair[] = [];
  let next = 0;
  const flushOld = (until: number) => {
    for (; next < until; next++) if (!oldUsed[next]) out.push([next, null]);
  };
  newToOld.forEach((i, j) => {
    if (i !== null) {
      flushOld(i);
      next = Math.max(next, i + 1);
    }
    out.push([i, j]);
  });
  flushOld(oldRows.length);
  return out;
}

/** One step of an edit script: rows equal on both sides, or a row only on one side. */
type Edit =
  { op: "eq"; a: number; b: number } | { op: "del"; a: number } | { op: "ins"; b: number };

/**
 * Myers' O(ND) shortest edit script between `a[from..from+n)` and `b[from..from+m)`, or `null`
 * when more than `maxD` rows differ. Keeps one slice of the frontier per step for the walk back,
 * so memory grows with the number of differences, not with the table size.
 */
function myers(
  a: string[],
  b: string[],
  from: number,
  n: number,
  m: number,
  maxD: number,
): Edit[] | null {
  const off = n + m + 1;
  const v = new Int32Array(2 * off + 1);
  const trace: Int32Array[] = [];
  let found = -1;
  for (let d = 0; d <= Math.min(maxD, n + m) && found < 0; d++) {
    trace.push(v.slice(off - d - 1, off + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let x =
        k === -d || (k !== d && v[off + k - 1]! < v[off + k + 1]!)
          ? v[off + k + 1]!
          : v[off + k - 1]! + 1;
      let y = x - k;
      while (x < n && y < m && a[from + x] === b[from + y]) {
        x += 1;
        y += 1;
      }
      v[off + k] = x;
      if (x >= n && y >= m) {
        found = d;
        break;
      }
    }
  }
  if (found < 0) return null;
  const edits: Edit[] = [];
  let x = n;
  let y = m;
  for (let d = found; d >= 0; d--) {
    const prev = trace[d]!;
    const at = (k: number) => prev[k + d + 1]!;
    const k = x - y;
    const prevK = k === -d || (k !== d && at(k - 1) < at(k + 1)) ? k + 1 : k - 1;
    const prevX = d === 0 ? 0 : at(prevK);
    const prevY = d === 0 ? 0 : prevX - prevK;
    while (x > prevX && y > prevY) {
      x -= 1;
      y -= 1;
      edits.push({ op: "eq", a: from + x, b: from + y });
    }
    if (d > 0) {
      if (x === prevX) edits.push({ op: "ins", b: from + prevY });
      else edits.push({ op: "del", a: from + prevX });
    }
    x = prevX;
    y = prevY;
  }
  return edits.reverse();
}

/** Most differing rows the position alignment looks for before pairing rows plainly in order. */
const MAX_EDIT_DISTANCE = 2000;

/**
 * Pairs rows by position: identical rows are aligned first (shortest edit script, after
 * trimming the common start and end), then the rows left between two aligned ones pair up in
 * order and the extra ones are added or removed. When too many rows differ the alignment is
 * skipped and rows pair in order.
 */
export function pairByPosition(oldSig: string[], newSig: string[]): RowPair[] {
  const out: RowPair[] = [];
  let pre = 0;
  while (pre < oldSig.length && pre < newSig.length && oldSig[pre] === newSig[pre]) {
    out.push([pre, pre]);
    pre += 1;
  }
  let suf = 0;
  while (
    suf < oldSig.length - pre &&
    suf < newSig.length - pre &&
    oldSig[oldSig.length - 1 - suf] === newSig[newSig.length - 1 - suf]
  ) {
    suf += 1;
  }
  const m = oldSig.length - pre - suf;
  const n = newSig.length - pre - suf;
  let ho: number[] = [];
  let hn: number[] = [];
  const flush = () => {
    const both = Math.min(ho.length, hn.length);
    for (let k = 0; k < both; k++) out.push([ho[k]!, hn[k]!]);
    for (let k = both; k < ho.length; k++) out.push([ho[k]!, null]);
    for (let k = both; k < hn.length; k++) out.push([null, hn[k]!]);
    ho = [];
    hn = [];
  };
  // Both middles start at `pre`, so `myers` takes one start index.
  const edits = m > 0 && n > 0 ? myers(oldSig, newSig, pre, m, n, MAX_EDIT_DISTANCE) : null;
  if (edits) {
    for (const e of edits) {
      if (e.op === "eq") {
        flush();
        out.push([e.a, e.b]);
      } else if (e.op === "del") ho.push(e.a);
      else hn.push(e.b);
    }
  } else {
    for (let k = 0; k < m; k++) ho.push(pre + k);
    for (let k = 0; k < n; k++) hn.push(pre + k);
  }
  flush();
  for (let k = 0; k < suf; k++) out.push([pre + m + k, pre + n + k]);
  return out;
}

export type RowKind = "same" | "changed" | "added" | "removed";

export interface TableRow {
  kind: RowKind;
  /** Record numbers in the file (1-based, the header row is record 1). */
  oldNo: number | null;
  newNo: number | null;
  old: string[] | null;
  new: string[] | null;
  /** Indexes into `columns` of the cells whose value changed (paired rows only). */
  changed: number[];
}

export interface TableStats {
  same: number;
  changed: number;
  added: number;
  removed: number;
  changedCells: number;
  addedColumns: number;
  removedColumns: number;
  renamedColumns: number;
}

export type NoiseReason = "lineEndings" | "quoting" | "whitespace" | "bom";

export interface ParsedSides {
  delimiter: Delimiter;
  oldText: string | null;
  newText: string | null;
  oldRows: string[][];
  newRows: string[][];
}

export type ParseResult = { ok: true; sides: ParsedSides } | { ok: false; reason: "tooLarge" };

/** Parses both sides, or says the table is too large to build. */
export function parseSides(
  path: string,
  oldText: string | null,
  newText: string | null,
): ParseResult {
  if ((oldText?.length ?? 0) > TABLE_MAX_CHARS || (newText?.length ?? 0) > TABLE_MAX_CHARS) {
    return { ok: false, reason: "tooLarge" };
  }
  const delimiter = detectDelimiter(path, newText || oldText || "");
  const oldRows = oldText === null ? [] : parseCsv(oldText, delimiter);
  const newRows = newText === null ? [] : parseCsv(newText, delimiter);
  if (oldRows.length > TABLE_MAX_ROWS || newRows.length > TABLE_MAX_ROWS) {
    return { ok: false, reason: "tooLarge" };
  }
  return { ok: true, sides: { delimiter, oldText, newText, oldRows, newRows } };
}

function sameRecords(a: string[][], b: string[][], trim: boolean): boolean {
  if (a.length !== b.length) return false;
  for (let r = 0; r < a.length; r++) {
    const x = a[r]!;
    const y = b[r]!;
    if (x.length !== y.length) return false;
    for (let c = 0; c < x.length; c++) {
      const p = x[c]!;
      const q = y[c]!;
      if (trim ? p.trim() !== q.trim() : p !== q) return false;
    }
  }
  return true;
}

/**
 * When the two texts differ but every value is the same (ignoring spaces around values), the
 * kinds of formatting that changed; `null` when values really differ or nothing changed.
 */
export function formatNoise(sides: ParsedSides): NoiseReason[] | null {
  const { oldText: a, newText: b, oldRows, newRows, delimiter } = sides;
  if (a === null || b === null || a === b) return null;
  const exact = sameRecords(oldRows, newRows, false);
  if (!exact && !sameRecords(oldRows, newRows, true)) return null;
  const bom = (t: string) => t.charCodeAt(0) === 0xfeff;
  const body = (t: string) => (bom(t) ? t.slice(1) : t);
  const crlf = (t: string) => t.includes("\r\n");
  const tail = (t: string) => (/(\r?\n|\r)*$/.exec(t)?.[0] ?? "").replace(/\r\n?/g, "\n").length;
  const norm = (t: string) => body(t).replace(/\r\n?/g, "\n").replace(/\n+$/, "");
  const spaces = delimiter === "\t" ? / +/g : /[ \t]+/g;
  const reasons: NoiseReason[] = [];
  if (crlf(a) !== crlf(b) || tail(a) !== tail(b)) reasons.push("lineEndings");
  if (norm(a).replace(spaces, "") !== norm(b).replace(spaces, "")) reasons.push("quoting");
  if (!exact) reasons.push("whitespace");
  if (bom(a) !== bom(b)) reasons.push("bom");
  if (reasons.length === 0) reasons.push("quoting");
  return reasons;
}

/** Key column choice: the default rule, row position, or a column (index into `columns`). */
export type KeyChoice = "auto" | "position" | number;

export interface TableDiffOptions {
  /** `null`: detect from the data. */
  header: boolean | null;
  key: KeyChoice;
}

export interface TableDiff {
  delimiter: Delimiter;
  header: boolean;
  headerDetected: boolean;
  columns: TableColumn[];
  /** Columns present on both sides, usable as a key (indexes into `columns`). */
  keyOptions: number[];
  /** Column used to pair rows, or `null` for row position. */
  key: number | null;
  /** Whether the key column's values are filled in and distinct on both sides. */
  keyUnique: boolean;
  rows: TableRow[];
  stats: TableStats;
  noise: NoiseReason[] | null;
  /** One side is missing (new or deleted file). */
  oneSided: boolean;
}

/** Builds the table diff of two parsed sides. */
export function diffTables(sides: ParsedSides, options: TableDiffOptions): TableDiff {
  const { oldRows, newRows } = sides;
  const oneSided = sides.oldText === null || sides.newText === null;
  const headerDetected = detectHeader(newRows.length > 0 ? newRows : oldRows);
  const header = options.header ?? headerDetected;
  const skip = header ? 1 : 0;
  const oldBody = oldRows.slice(skip);
  const newBody = newRows.slice(skip);
  const oldWidth = width(oldRows);
  const newWidth = width(newRows);
  // A new or deleted file: its columns are neither added nor removed, just there.
  const columns = oneSided
    ? matchColumns(
        header ? (oldRows[0] ?? newRows[0] ?? null) : null,
        header ? (newRows[0] ?? oldRows[0] ?? null) : null,
        Math.max(oldWidth, newWidth),
        Math.max(oldWidth, newWidth),
      )
    : matchColumns(
        header ? (oldRows[0] ?? []) : null,
        header ? (newRows[0] ?? []) : null,
        oldWidth,
        newWidth,
      );
  // Columns on both sides: index into `columns`, and the cell index on each side.
  const pairsOfCols: { ci: number; oi: number; ni: number }[] = [];
  columns.forEach((c, ci) => {
    if (c.oldIndex !== null && c.newIndex !== null) {
      pairsOfCols.push({ ci, oi: c.oldIndex, ni: c.newIndex });
    }
  });
  const shared = pairsOfCols.map((c) => c.ci);
  const sharedCol = (ci: number) => pairsOfCols.find((c) => c.ci === ci);

  const unique = (ci: number) => {
    const c = sharedCol(ci);
    return !!c && isUniqueColumn(oldBody, c.oi) && isUniqueColumn(newBody, c.ni);
  };
  let key: number | null = null;
  const first = shared[0];
  if (typeof options.key === "number" && shared.includes(options.key)) key = options.key;
  else if (options.key === "auto" && !oneSided && first !== undefined && unique(first)) {
    key = first;
  }
  const keyUnique = key !== null && unique(key);
  const keyCol = key === null ? undefined : sharedCol(key);

  // When the only differences are formatting, spaces around values do not count as changes,
  // so the banner and the rows agree. Otherwise every character counts.
  const noise = formatNoise(sides);
  const trim = !!noise?.includes("whitespace");
  const value = (v: string | undefined) => (trim ? (v ?? "").trim() : (v ?? ""));

  let pairs: RowPair[];
  if (oneSided) {
    pairs =
      sides.oldText === null ? newBody.map((_, j) => [null, j]) : oldBody.map((_, i) => [i, null]);
  } else if (keyCol) {
    pairs = pairByKey(oldBody, newBody, keyCol.oi, keyCol.ni);
  } else {
    const sig = (rows: string[][], side: "oi" | "ni") =>
      rows.map((r) => pairsOfCols.map((c) => value(r[c[side]])).join("\u001f"));
    pairs = pairByPosition(sig(oldBody, "oi"), sig(newBody, "ni"));
  }

  const stats: TableStats = {
    same: 0,
    changed: 0,
    added: 0,
    removed: 0,
    changedCells: 0,
    addedColumns: oneSided ? 0 : columns.filter((c) => c.status === "added").length,
    removedColumns: oneSided ? 0 : columns.filter((c) => c.status === "removed").length,
    renamedColumns: oneSided ? 0 : columns.filter((c) => c.status === "renamed").length,
  };
  const rows: TableRow[] = pairs.map(([i, j]) => {
    const old = i === null ? null : (oldBody[i] ?? null);
    const nw = j === null ? null : (newBody[j] ?? null);
    const oldNo = i === null ? null : i + skip + 1;
    const newNo = j === null ? null : j + skip + 1;
    if (!old || !nw) {
      stats[old ? "removed" : "added"] += 1;
      return { kind: old ? "removed" : "added", oldNo, newNo, old, new: nw, changed: [] };
    }
    const changed = pairsOfCols
      .filter((c) => value(old[c.oi]) !== value(nw[c.ni]))
      .map((c) => c.ci);
    stats.changedCells += changed.length;
    stats[changed.length > 0 ? "changed" : "same"] += 1;
    return { kind: changed.length > 0 ? "changed" : "same", oldNo, newNo, old, new: nw, changed };
  });

  return {
    delimiter: sides.delimiter,
    header,
    headerDetected,
    columns,
    keyOptions: shared,
    key,
    keyUnique,
    rows,
    stats,
    noise,
    oneSided,
  };
}

/** What a table cell shows: the value on each side for one column of one row. */
export function cellValues(
  row: TableRow,
  col: TableColumn,
): { old: string | null; new: string | null } {
  return {
    old: row.old && col.oldIndex !== null ? (row.old[col.oldIndex] ?? "") : null,
    new: row.new && col.newIndex !== null ? (row.new[col.newIndex] ?? "") : null,
  };
}

/** A displayed line: a row, or a run of hidden unchanged rows. */
export type DisplayItem = { row: TableRow } | { hidden: number };

/** The rows to show; with `changesOnly`, runs of unchanged rows collapse into one marker. */
export function displayItems(rows: TableRow[], changesOnly: boolean): DisplayItem[] {
  if (!changesOnly) return rows.map((row) => ({ row }));
  const out: DisplayItem[] = [];
  let run = 0;
  for (const row of rows) {
    if (row.kind === "same") {
      run += 1;
      continue;
    }
    if (run > 0) out.push({ hidden: run });
    run = 0;
    out.push({ row });
  }
  if (run > 0) out.push({ hidden: run });
  return out;
}
