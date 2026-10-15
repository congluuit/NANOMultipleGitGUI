import type { GraphRow } from "@/ipc";
import { readToken } from "@/theme/theme";

export const ROW_HEIGHT = 28;
export const LANE_WIDTH = 14;
const GRAPH_PAD = 10;
const NODE_RADIUS = 5;
const AUTHOR_WIDTH = 150;
const DATE_WIDTH = 130;
export const MAX_DRAWN_LANES = 24;
const MIN_GRAPH_WIDTH = 120;

export interface Palette {
  lanes: string[];
  bg: string;
  surface2: string;
  primary: string;
  text: string;
  muted: string;
  border: string;
  font: string;
  mono: string;
}

export function readPalette(el: Element = document.documentElement): Palette {
  const style = getComputedStyle(el);
  return {
    lanes: Array.from({ length: 8 }, (_, i) => readToken(`--lane-${i}`, el)),
    bg: readToken("--bg", el),
    surface2: readToken("--surface-2", el),
    primary: readToken("--primary", el),
    text: readToken("--text", el),
    muted: readToken("--text-muted", el),
    border: readToken("--border", el),
    font: style.getPropertyValue("--font-sans").trim() || "sans-serif",
    mono: style.getPropertyValue("--font-mono").trim() || "monospace",
  };
}

/** Width of the graph column; never narrower than its header label. */
export function graphWidth(maxLanes: number): number {
  return Math.max(
    MIN_GRAPH_WIDTH,
    Math.min(Math.max(maxLanes, 1), MAX_DRAWN_LANES) * LANE_WIDTH + GRAPH_PAD * 2,
  );
}

/** Room the message column keeps before the author / date columns give way. */
const MIN_MESSAGE_WIDTH = 180;
const MIN_AUTHOR_WIDTH = 100;
/** Gap between the graph column and the message text. */
const TEXT_GAP = 8;
/** With default widths the message stays this many times wider than the author / date. */
const MESSAGE_RATIO = 1.5;

/** Widths the user dragged a column to (`null` / missing = default). */
export interface ColumnWidths {
  graph?: number | null;
  author?: number | null;
  date?: number | null;
}

export const COLUMN_LIMITS = {
  graph: { min: 48, max: 640 },
  author: { min: 60, max: 420 },
  date: { min: 80, max: 260 },
} as const;

export const defaultColumnWidths = (maxLanes: number) => ({
  graph: graphWidth(maxLanes),
  author: AUTHOR_WIDTH,
  date: DATE_WIDTH,
});

/** Horizontal layout shared by the canvas, the column header and hit-testing. */
export interface ColumnLayout {
  graph: number;
  /** Left edge of lane 0: the lanes are centered in the graph column. */
  laneOffset: number;
  /** Left edge of the message column (ref labels start there). */
  textX: number;
  /** Message text is clipped here (before the author column). */
  messageEnd: number;
  authorX: number;
  /** 0 = hidden. */
  author: number;
  dateX: number;
  /** 0 = hidden. */
  date: number;
}

const clampTo = (v: number, { min, max }: { min: number; max: number }) =>
  Math.round(Math.min(max, Math.max(min, v)));

/**
 * Column layout for a graph `width` px wide. The message column takes what is left and, while
 * the author / date widths are the defaults, stays the widest. In a narrow window the author
 * column shrinks, then hides, then the date hides, so the message stays readable.
 */
export function columnLayout(
  width: number,
  maxLanes: number,
  widths: ColumnWidths = {},
): ColumnLayout {
  const graph = clampTo(widths.graph ?? graphWidth(maxLanes), COLUMN_LIMITS.graph);
  const lanes = Math.min(Math.max(maxLanes, 1), MAX_DRAWN_LANES) * LANE_WIDTH;
  const laneOffset = Math.max(GRAPH_PAD, Math.floor((graph - lanes) / 2));
  const textX = graph + TEXT_GAP;
  const custom = Boolean(widths.author || widths.date);
  const wantAuthor = widths.author ? clampTo(widths.author, COLUMN_LIMITS.author) : AUTHOR_WIDTH;
  const wantDate = widths.date ? clampTo(widths.date, COLUMN_LIMITS.date) : DATE_WIDTH;
  // Room for message + author + date.
  const avail = width - textX;
  const minMessage = (other: number) =>
    custom ? MIN_MESSAGE_WIDTH : Math.max(MIN_MESSAGE_WIDTH, other * MESSAGE_RATIO);

  let author = 0;
  let date = 0;
  if (width <= 0 || avail - wantAuthor - wantDate >= minMessage(Math.max(wantAuthor, wantDate))) {
    author = wantAuthor;
    date = wantDate;
  } else {
    // Shrink the author column (by default never past the message width).
    const shrunk = Math.min(
      wantAuthor,
      custom
        ? avail - wantDate - MIN_MESSAGE_WIDTH
        : Math.floor((avail - wantDate) / (1 + MESSAGE_RATIO)),
    );
    if (shrunk >= MIN_AUTHOR_WIDTH && avail - wantDate - shrunk >= MIN_MESSAGE_WIDTH) {
      author = shrunk;
      date = wantDate;
    } else if (avail - wantDate >= minMessage(wantDate)) {
      date = wantDate;
    }
  }
  const dateX = width - date;
  const authorX = dateX - author;
  return { graph, laneOffset, textX, messageEnd: authorX - 12, authorX, author, dateX, date };
}

export const laneX = (lane: number, offset: number = GRAPH_PAD) =>
  offset + lane * LANE_WIDTH + LANE_WIDTH / 2;

export interface RefLabel {
  text: string;
  kind: "head" | "branch" | "remote" | "tag";
}

/** Turns full `%D` decorations into labels (`HEAD -> refs/heads/main` → head "main"). */
export function parseRefs(refs: string[]): RefLabel[] {
  const strip = (ref: string, prefix: string) =>
    ref.startsWith(prefix) ? ref.slice(prefix.length) : ref;
  return refs.map<RefLabel>((r) => {
    if (r === "HEAD") return { text: "HEAD", kind: "head" };
    if (r.startsWith("HEAD -> ")) return { text: strip(r.slice(8), "refs/heads/"), kind: "head" };
    if (r.startsWith("tag: ")) return { text: strip(r.slice(5), "refs/tags/"), kind: "tag" };
    if (r.startsWith("refs/remotes/")) return { text: strip(r, "refs/remotes/"), kind: "remote" };
    return { text: strip(r, "refs/heads/"), kind: "branch" };
  });
}

/** Font of the ref labels (also used to measure them for hit-testing). */
export const labelFont = (palette: Pick<Palette, "font">) => `600 11px ${palette.font}`;

export interface LabelBox {
  label: RefLabel;
  x: number;
  width: number;
}

/** Horizontal layout of a row's ref labels, as drawn (`measure` = text width in px). */
export function refLabelBoxes(
  labels: RefLabel[],
  startX: number,
  measure: (text: string) => number,
): LabelBox[] {
  let x = startX;
  return labels.map((label) => {
    const width = measure(label.text) + 12;
    const box = { label, x, width };
    x += width + 4;
    return box;
  });
}

/** The label under `x`, if any. */
export const labelAt = (boxes: LabelBox[], x: number) =>
  boxes.find((b) => x >= b.x && x <= b.x + b.width)?.label ?? null;

export function formatDate(seconds: number, locale: string): string {
  return new Date(seconds * 1000).toLocaleString(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export type GraphCell = "graph" | "message" | "author" | "date";

/** Column under `x` (canvas pixels). */
export function cellAt(cols: ColumnLayout, x: number): GraphCell | null {
  if (x < cols.graph) return "graph";
  if (x >= cols.textX && x < cols.authorX) return "message";
  if (cols.author > 0 && x >= cols.authorX && x < cols.dateX) return "author";
  if (cols.date > 0 && x >= cols.dateX) return "date";
  return null;
}

export interface TextMeasure {
  /** Ref label font ([`labelFont`]). */
  label: (text: string) => number;
  /** Subject / author font (12 px sans). */
  text: (text: string) => number;
  /** Date font (11 px mono). */
  date: (text: string) => number;
  /** Width of the [`DrawParams.decorate`] mark of a row (0 / missing = none). */
  mark?: (row: GraphRow) => number;
}

/** Where a row's subject — or its [`DrawParams.decorate`] mark — starts, after the ref labels. */
export function afterLabelsX(row: GraphRow, textX: number, measureLabel: (t: string) => number) {
  const boxes = refLabelBoxes(parseRefs(row.refs), textX, measureLabel);
  const last = boxes[boxes.length - 1];
  return last ? last.x + last.width + 8 : textX;
}

/**
 * Tooltip of a cell: its full text when the canvas had to cut it, plus — on the message — the
 * author and date while their columns are hidden in a narrow window. `null` = nothing hidden.
 */
export function cellTooltip(
  row: GraphRow,
  cell: GraphCell,
  cols: ColumnLayout,
  measure: TextMeasure,
  locale: string,
): string | null {
  const author = `${row.authorName} <${row.authorEmail}>`;
  const date = formatDate(row.authorTime, locale);
  if (cell === "message") {
    const labelsEnd = afterLabelsX(row, cols.textX, measure.label);
    const mark = measure.mark?.(row) ?? 0;
    const subjectStart = labelsEnd + (mark > 0 ? mark + DECORATION_GAP : 0);
    const lines: string[] = [];
    if (subjectStart + measure.text(row.subject) > cols.messageEnd) {
      lines.push(row.subject);
      if (labelsEnd > cols.messageEnd)
        lines.push(
          parseRefs(row.refs)
            .map((l) => l.text)
            .join(", "),
        );
    }
    if (cols.author === 0) lines.push(author);
    if (cols.date === 0) lines.push(date);
    return lines.length > 0 ? lines.join("\n") : null;
  }
  if (cell === "author") {
    return measure.text(row.authorName) > cols.author - 12 ? author : null;
  }
  if (cell === "date") return measure.date(date) > cols.date ? date : null;
  return null;
}

export interface DrawParams {
  ctx: CanvasRenderingContext2D;
  width: number;
  height: number;
  scrollTop: number;
  total: number;
  maxLanes: number;
  getRow: (index: number) => GraphRow | undefined;
  palette: Palette;
  selected: number | null;
  hovered: number | null;
  /** Search highlighting: "current" for the active match, "match" for the others. */
  highlight?: (index: number) => "match" | "current" | null;
  locale: string;
  /** Branch label outlined while another branch is dragged over it. */
  dropTarget?: { index: number; branch: string } | null;
  /** Column widths the user chose (defaults otherwise). */
  columns?: ColumnWidths;
  /**
   * Draws a mark (e.g. the CI status) between the ref labels and the subject, starting at `x`
   * and centered on `cy`; returns the width it used (0 = nothing drawn).
   */
  decorate?: (ctx: CanvasRenderingContext2D, x: number, cy: number, row: GraphRow) => number;
}

/** Space between a [`DrawParams.decorate`] mark and the subject. */
export const DECORATION_GAP = 6;

function curve(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
  ctx.moveTo(x1, y1);
  if (x1 === x2) {
    ctx.lineTo(x2, y2);
  } else {
    const mid = (y1 + y2) / 2;
    ctx.bezierCurveTo(x1, mid, x2, mid, x2, y2);
  }
}

/** Draws the visible window. Returns how many visible rows were not loaded yet. */
export function drawGraph(p: DrawParams): number {
  const { ctx, width, height, scrollTop, palette } = p;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT));
  const last = Math.min(p.total, Math.ceil((scrollTop + height) / ROW_HEIGHT));
  const cols = columnLayout(width, p.maxLanes, p.columns);
  const { textX, authorX, dateX } = cols;
  const lx = (lane: number) => laneX(lane, cols.laneOffset);
  let missing = 0;

  ctx.clearRect(0, 0, width, height);
  ctx.textBaseline = "middle";

  for (let i = first; i < last; i++) {
    const y0 = i * ROW_HEIGHT - scrollTop;
    const cy = y0 + ROW_HEIGHT / 2;
    if (i === p.selected || i === p.hovered) {
      ctx.fillStyle = palette.surface2;
      ctx.globalAlpha = i === p.selected ? 1 : 0.6;
      ctx.fillRect(0, y0, width, ROW_HEIGHT);
      ctx.globalAlpha = 1;
    }
    const mark = p.highlight?.(i);
    if (mark) {
      ctx.fillStyle = palette.primary;
      ctx.globalAlpha = mark === "current" ? 0.22 : 0.09;
      ctx.fillRect(0, y0, width, ROW_HEIGHT);
      ctx.globalAlpha = 1;
    }
    if (i === p.selected) {
      ctx.fillStyle = palette.primary;
      ctx.fillRect(0, y0, 3, ROW_HEIGHT);
    }
    const row = p.getRow(i);
    if (!row) {
      missing++;
      ctx.fillStyle = palette.surface2;
      ctx.fillRect(textX, cy - 4, Math.min(320, authorX - textX - 16), 8);
      continue;
    }

    // Graph lines: upper half then lower half, clipped to the graph column.
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, y0, cols.graph, ROW_HEIGHT);
    ctx.clip();
    ctx.lineWidth = 2;
    const s = row.segments;
    for (let k = 0; k < s.length; k += 4) {
      const [kind, from, to, color] = [s[k]!, s[k + 1]!, s[k + 2]!, s[k + 3]!];
      if (from >= MAX_DRAWN_LANES && to >= MAX_DRAWN_LANES) continue;
      ctx.strokeStyle = palette.lanes[color % 8]!;
      ctx.beginPath();
      if (kind === 0) curve(ctx, lx(from), y0, lx(to), cy);
      else curve(ctx, lx(from), cy, lx(to), y0 + ROW_HEIGHT);
      ctx.stroke();
    }

    // Node.
    const nx = lx(Math.min(row.lane, MAX_DRAWN_LANES - 1));
    const laneColor = palette.lanes[row.color % 8]!;
    ctx.beginPath();
    ctx.arc(nx, cy, NODE_RADIUS, 0, Math.PI * 2);
    ctx.fillStyle = row.isMerge ? palette.bg : laneColor;
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = row.isMerge ? laneColor : palette.bg;
    ctx.stroke();
    ctx.restore();

    // Ref labels + subject, clipped before the author column.
    ctx.save();
    ctx.beginPath();
    ctx.rect(textX, y0, Math.max(0, cols.messageEnd - textX), ROW_HEIGHT);
    ctx.clip();
    let x = textX;
    ctx.font = labelFont(palette);
    const dragOver = p.dropTarget?.index === i ? p.dropTarget.branch : null;
    for (const { label: ref, x: bx, width: w } of refLabelBoxes(
      parseRefs(row.refs),
      textX,
      (text) => ctx.measureText(text).width,
    )) {
      x = bx;
      if (dragOver === ref.text && ref.kind !== "remote" && ref.kind !== "tag") {
        ctx.strokeStyle = palette.primary;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.roundRect(x - 2, cy - 11, w + 4, 22, 11);
        ctx.stroke();
      }
      ctx.fillStyle = laneColor;
      ctx.globalAlpha = ref.kind === "remote" ? 0.12 : 0.2;
      ctx.beginPath();
      ctx.roundRect(x, cy - 9, w, 18, ref.kind === "tag" ? 2 : 9);
      ctx.fill();
      ctx.globalAlpha = 1;
      if (ref.kind === "head") {
        ctx.strokeStyle = laneColor;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
      ctx.fillStyle = palette.text;
      ctx.fillText(ref.text, x + 6, cy + 0.5);
      x += w + 4;
    }
    let subjectX = x + (x > textX ? 4 : 0);
    const used = p.decorate?.(ctx, subjectX, cy, row) ?? 0;
    if (used > 0) subjectX += used + DECORATION_GAP;
    ctx.font = `12px ${palette.font}`;
    ctx.fillStyle = row.isMerge ? palette.muted : palette.text;
    ctx.fillText(row.subject, subjectX, cy + 0.5);
    ctx.restore();

    if (cols.author > 0) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(authorX, y0, cols.author - 12, ROW_HEIGHT);
      ctx.clip();
      ctx.font = `12px ${palette.font}`;
      ctx.fillStyle = palette.muted;
      ctx.fillText(row.authorName, authorX, cy + 0.5);
      ctx.restore();
    }

    if (cols.date > 0) {
      ctx.font = `11px ${palette.mono}`;
      ctx.fillStyle = palette.muted;
      ctx.fillText(formatDate(row.authorTime, p.locale), dateX, cy + 0.5);
    }
  }
  return missing;
}
