import type { GraphRow } from "@/ipc";
import { readToken } from "@/theme/theme";
import { contrastText, initialsBadge } from "./avatars";

export const ROW_HEIGHT = 28;
export const LANE_WIDTH = 22;
const GRAPH_PAD = 10;
/** Merge commits: small hollow dot. */
const DOT_RADIUS = 4.5;
/** Other commits: the author's avatar, ringed with the lane color. */
export const AVATAR_RADIUS = 11.5;
const AVATAR_INNER = 9.5;
const REFS_WIDTH = 150;
const AUTHOR_WIDTH = 150;
const DATE_WIDTH = 130;
export const MAX_DRAWN_LANES = 24;
const MIN_GRAPH_WIDTH = 80;

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
  refs?: number | null;
  graph?: number | null;
  author?: number | null;
  date?: number | null;
}

export const COLUMN_LIMITS = {
  refs: { min: 60, max: 400 },
  graph: { min: 48, max: 640 },
  author: { min: 60, max: 420 },
  date: { min: 80, max: 260 },
} as const;

export const defaultColumnWidths = (maxLanes: number) => ({
  refs: REFS_WIDTH,
  graph: graphWidth(maxLanes),
  author: AUTHOR_WIDTH,
  date: DATE_WIDTH,
});

/** Horizontal layout shared by the canvas, the column header and hit-testing. */
export interface ColumnLayout {
  /** Width of the "Branch / Tag" column (always at x = 0). */
  refs: number;
  /** Left edge of the graph column. */
  graphX: number;
  /** Width of the graph column. */
  graph: number;
  /** Left edge of lane 0 (canvas x): the lanes are centered in the graph column. */
  laneOffset: number;
  /** Left edge of the message column. */
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
 * Column layout for a graph `width` px wide: refs | graph | message | author | date. The message
 * column takes what is left and, while the author / date widths are the defaults, stays the
 * widest. In a narrow window the author column shrinks, then hides, then the date hides, so the
 * message stays readable.
 */
export function columnLayout(
  width: number,
  maxLanes: number,
  widths: ColumnWidths = {},
): ColumnLayout {
  const refs = clampTo(widths.refs ?? REFS_WIDTH, COLUMN_LIMITS.refs);
  const graphX = refs;
  const graph = clampTo(widths.graph ?? graphWidth(maxLanes), COLUMN_LIMITS.graph);
  const lanes = Math.min(Math.max(maxLanes, 1), MAX_DRAWN_LANES) * LANE_WIDTH;
  const laneOffset = graphX + Math.max(GRAPH_PAD, Math.floor((graph - lanes) / 2));
  const textX = graphX + graph + TEXT_GAP;
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
  return {
    refs,
    graphX,
    graph,
    laneOffset,
    textX,
    messageEnd: authorX - 12,
    authorX,
    author,
    dateX,
    date,
  };
}

/** Center of `lane`; `offset` = [`ColumnLayout.laneOffset`]. */
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

/** One pill of the "Branch / Tag" column: a ref, or a local branch merged with its remotes. */
export interface RefPill {
  /** Label for hit-testing, drag and drop and menus: the local branch of a merged pill. */
  label: RefLabel;
  /** Text shown. */
  name: string;
  /** Checked-out branch (or detached `HEAD`). */
  head: boolean;
  /** Has a local branch (laptop icon). */
  local: boolean;
  /** Has a remote-tracking branch (cloud icon). */
  remote: boolean;
  tag: boolean;
  /** Every ref shown by the pill. */
  refs: RefLabel[];
}

const remoteOf = (text: string) => text.slice(0, Math.max(0, text.indexOf("/")));
const remoteBranch = (text: string) => text.slice(text.indexOf("/") + 1);

/**
 * Groups a row's labels into pills: HEAD first, then local branches, remotes, tags. A local
 * branch and the remote-tracking branches of the same name (`main` + `origin/main`) share one
 * pill; `<remote>/HEAD` joins the pill of another branch of that remote.
 */
export function groupRefs(labels: RefLabel[]): RefPill[] {
  const locals = [
    ...labels.filter((l) => l.kind === "head"),
    ...labels.filter((l) => l.kind === "branch"),
  ];
  const remotes = labels.filter((l) => l.kind === "remote");
  const used = new Set<RefLabel>();
  const pills: RefPill[] = [];
  for (const l of locals) {
    const detached = l.kind === "head" && l.text === "HEAD";
    const merged = detached
      ? []
      : remotes.filter(
          (r) => !used.has(r) && r.text.includes("/") && remoteBranch(r.text) === l.text,
        );
    for (const r of merged) used.add(r);
    pills.push({
      label: l,
      name: l.text,
      head: l.kind === "head",
      local: !detached,
      remote: merged.length > 0,
      tag: false,
      refs: [l, ...merged],
    });
  }
  const remoteHeads: RefLabel[] = [];
  for (const r of remotes) {
    if (used.has(r)) continue;
    if (remoteBranch(r.text) === "HEAD" && r.text.includes("/")) {
      remoteHeads.push(r);
      continue;
    }
    pills.push({
      label: r,
      name: r.text,
      head: false,
      local: false,
      remote: true,
      tag: false,
      refs: [r],
    });
  }
  for (const r of remoteHeads) {
    const owner = pills.find((p) =>
      p.refs.some((x) => x.kind === "remote" && remoteOf(x.text) === remoteOf(r.text)),
    );
    if (owner) owner.refs.push(r);
    else
      pills.push({
        label: r,
        name: r.text,
        head: false,
        local: false,
        remote: true,
        tag: false,
        refs: [r],
      });
  }
  for (const t of labels) {
    if (t.kind !== "tag") continue;
    pills.push({
      label: t,
      name: t.text,
      head: false,
      local: false,
      remote: false,
      tag: true,
      refs: [t],
    });
  }
  return pills;
}

const NO_PILLS: RefPill[] = [];
const pillCache = new WeakMap<GraphRow, RefPill[]>();

/** [`groupRefs`] of a row, cached per row object. */
export function rowPills(row: GraphRow): RefPill[] {
  if (row.refs.length === 0) return NO_PILLS;
  let pills = pillCache.get(row);
  if (!pills) {
    pills = groupRefs(parseRefs(row.refs));
    pillCache.set(row, pills);
  }
  return pills;
}

/** Font of the ref labels (also used to measure them for hit-testing). */
export const labelFont = (palette: Pick<Palette, "font">) => `600 11px ${palette.font}`;

/** Longest prefix of `text` + "…" that fits `max` px (`text` itself when it fits). */
export function fitText(text: string, max: number, measure: (t: string) => number): string {
  if (measure(text) <= max) return text;
  const chars = Array.from(text);
  let lo = 0;
  let hi = chars.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (measure(chars.slice(0, mid).join("") + "…") <= max) lo = mid;
    else hi = mid - 1;
  }
  return lo === 0 ? "…" : chars.slice(0, lo).join("") + "…";
}

/** Text widths (and ellipsized texts) of one font, kept across frames. */
export class TextWidthCache {
  private widths = new Map<string, number>();
  private fits = new Map<string, string>();
  constructor(readonly font: string) {}

  measure(ctx: CanvasRenderingContext2D, text: string): number {
    let w = this.widths.get(text);
    if (w === undefined) {
      if (this.widths.size > 4096) this.widths.clear();
      ctx.font = this.font;
      w = ctx.measureText(text).width;
      this.widths.set(text, w);
    }
    return w;
  }

  fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
    const key = `${Math.floor(max)}\u0000${text}`;
    let out = this.fits.get(key);
    if (out === undefined) {
      if (this.fits.size > 2048) this.fits.clear();
      out = fitText(text, Math.floor(max), (t) => this.measure(ctx, t));
      this.fits.set(key, out);
    }
    return out;
  }
}

// Geometry of the ref pills.
const CELL_PAD = 6;
const PILL_PAD = 7;
const PILL_HEIGHT = 18;
const ICON_GAP = 4;
const CHECK_W = 8;
const TAG_W = 9;
const LAPTOP_W = 12;
const CLOUD_W = 13;
const MORE_GAP = 4;
const MORE_PAD = 5;

/** What the "Branch / Tag" cell of a row shows: the first pill and a "+N" counter. */
export interface RefCell {
  pills: RefPill[];
  pill: RefPill;
  x: number;
  width: number;
  /** Name as drawn (ellipsized when the column is narrow). */
  name: string;
  nameX: number;
  /** Pills not shown (0 = no counter). */
  more: number;
  moreText: string;
  moreX: number;
  moreWidth: number;
  /** Right edge of what is drawn (the connector to the node starts here). */
  end: number;
}

/**
 * Layout of a row's "Branch / Tag" cell (`measure` = width in the label font, `fit` = text
 * ellipsized to a width). `null` when the row has no refs.
 */
export function refCell(
  pills: RefPill[],
  cols: Pick<ColumnLayout, "refs">,
  measure: (text: string) => number,
  fit: (text: string, max: number) => string = (t, max) => fitText(t, max, measure),
): RefCell | null {
  const pill = pills[0];
  if (!pill) return null;
  const more = pills.length - 1;
  const moreText = more > 0 ? `+${more}` : "";
  const moreWidth = more > 0 ? measure(moreText) + MORE_PAD * 2 : 0;
  const lead = (pill.head ? CHECK_W + ICON_GAP : 0) + (pill.tag ? TAG_W + ICON_GAP : 0);
  const trail = (pill.local ? ICON_GAP + LAPTOP_W : 0) + (pill.remote ? ICON_GAP + CLOUD_W : 0);
  const fixed = PILL_PAD * 2 + lead + trail;
  const avail = cols.refs - CELL_PAD * 2 - (more > 0 ? moreWidth + MORE_GAP : 0);
  const maxName = Math.max(0, avail - fixed);
  let name = pill.name;
  let nameWidth = measure(name);
  if (nameWidth > maxName) {
    name = fit(pill.name, maxName);
    nameWidth = Math.min(maxName, measure(name));
  }
  const x = CELL_PAD;
  const width = fixed + nameWidth;
  const moreX = x + width + MORE_GAP;
  return {
    pills,
    pill,
    x,
    width,
    name,
    nameX: x + PILL_PAD + lead,
    more,
    moreText,
    moreX,
    moreWidth,
    end: more > 0 ? moreX + moreWidth : x + width,
  };
}

export interface LabelBox {
  label: RefLabel;
  pill: RefPill;
  x: number;
  width: number;
}

/** Hit boxes of a row's drawn pills (`measure` = text width in the label font). */
export function refLabelBoxes(
  pills: RefPill[],
  cols: Pick<ColumnLayout, "refs">,
  measure: (text: string) => number,
): LabelBox[] {
  const cell = refCell(pills, cols, measure);
  return cell ? [{ label: cell.pill.label, pill: cell.pill, x: cell.x, width: cell.width }] : [];
}

/** The label under `x`, if any. */
export const labelAt = (boxes: LabelBox[], x: number) =>
  boxes.find((b) => x >= b.x && x <= b.x + b.width)?.label ?? null;

/**
 * Ref under canvas `x` on `row` (any kind; the local branch of a merged pill), or `null`.
 * The "+N" counter is not a ref.
 */
export function refAtPoint(
  row: GraphRow,
  cols: Pick<ColumnLayout, "refs">,
  x: number,
  measure: (text: string) => number,
): RefLabel | null {
  if (x < 0 || x >= cols.refs) return null;
  return labelAt(refLabelBoxes(rowPills(row), cols, measure), x);
}

/** Tooltip of the "Branch / Tag" cell: every ref of the row, one pill per line. */
export function refsTooltip(pills: RefPill[]): string | null {
  if (pills.length === 0) return null;
  return pills
    .map(
      (p) =>
        (p.head ? "✓ " : "") +
        p.refs.map((r) => (r.kind === "tag" ? `tag: ${r.text}` : r.text)).join(", "),
    )
    .join("\n");
}

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

const dateCache = new WeakMap<GraphRow, { locale: string; text: string }>();

/** [`formatDate`] of a row's author time, cached per row (formatting is slow-ish). */
function rowDate(row: GraphRow, locale: string): string {
  let hit = dateCache.get(row);
  if (!hit || hit.locale !== locale) {
    hit = { locale, text: formatDate(row.authorTime, locale) };
    dateCache.set(row, hit);
  }
  return hit.text;
}

export type GraphCell = "refs" | "graph" | "message" | "author" | "date";

/** Column under `x` (canvas pixels). */
export function cellAt(cols: ColumnLayout, x: number): GraphCell | null {
  if (x < 0) return null;
  if (x < cols.graphX) return "refs";
  if (x < cols.graphX + cols.graph) return "graph";
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
}

/**
 * Tooltip of a cell: its full text when the canvas had to cut it, plus — on the message — the
 * author and date while their columns are hidden in a narrow window; on the "Branch / Tag"
 * cell, every ref of the row. `null` = nothing to show.
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
  if (cell === "refs") return refsTooltip(rowPills(row));
  if (cell === "message") {
    const lines: string[] = [];
    if (cols.textX + measure.text(row.subject) > cols.messageEnd) lines.push(row.subject);
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
  /** Loaded avatar of an author email (`null` = draw the initials). */
  avatar?: (email: string) => CanvasImageSource | null;
  /** Label-font measurements kept across frames (must use [`labelFont`] of `palette`). */
  labelWidths?: TextWidthCache;
}

function curve(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
  ctx.moveTo(x1, y1);
  if (x1 === x2) {
    ctx.lineTo(x2, y2);
  } else {
    const mid = (y1 + y2) / 2;
    ctx.bezierCurveTo(x1, mid, x2, mid, x2, y2);
  }
}

const TAU = Math.PI * 2;

function drawCheck(ctx: CanvasRenderingContext2D, x: number, cy: number) {
  ctx.beginPath();
  ctx.moveTo(x, cy);
  ctx.lineTo(x + 3, cy + 3);
  ctx.lineTo(x + CHECK_W, cy - 3.5);
  ctx.lineWidth = 1.8;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.stroke();
  ctx.lineCap = "butt";
  ctx.lineJoin = "miter";
}

/** Laptop: screen outline + base. */
function drawLaptop(ctx: CanvasRenderingContext2D, x: number, cy: number) {
  ctx.lineWidth = 1.3;
  ctx.beginPath();
  ctx.roundRect(x + 1.5, cy - 4.5, LAPTOP_W - 3, 6.5, 1);
  ctx.stroke();
  ctx.beginPath();
  ctx.roundRect(x, cy + 3, LAPTOP_W, 1.6, 0.8);
  ctx.fill();
}

/** Cloud: three overlapping circles on a flat base. */
function drawCloud(ctx: CanvasRenderingContext2D, x: number, cy: number) {
  ctx.beginPath();
  ctx.arc(x + 3.6, cy + 1.2, 2.9, 0, TAU);
  ctx.moveTo(x + 10.7, cy - 0.6);
  ctx.arc(x + 7, cy - 0.6, 3.7, 0, TAU);
  ctx.moveTo(x + 12.4, cy + 1.6);
  ctx.arc(x + 10.2, cy + 1.6, 2.2, 0, TAU);
  ctx.rect(x + 3.6, cy + 1.2, 6.6, 2.6);
  ctx.fill();
}

/** Tag: a label shape with a hole (`hole` = pill color). */
function drawTag(ctx: CanvasRenderingContext2D, x: number, cy: number, hole: string) {
  ctx.beginPath();
  ctx.moveTo(x, cy - 3.5);
  ctx.lineTo(x + 5, cy - 3.5);
  ctx.lineTo(x + TAG_W, cy);
  ctx.lineTo(x + 5, cy + 3.5);
  ctx.lineTo(x, cy + 3.5);
  ctx.closePath();
  ctx.fill();
  const fill = ctx.fillStyle;
  ctx.fillStyle = hole;
  ctx.beginPath();
  ctx.arc(x + 2.3, cy, 1.1, 0, TAU);
  ctx.fill();
  ctx.fillStyle = fill;
}

/** Draws the visible window. Returns how many visible rows were not loaded yet. */
export function drawGraph(p: DrawParams): number {
  const { ctx, width, height, scrollTop, palette } = p;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT));
  const last = Math.min(p.total, Math.ceil((scrollTop + height) / ROW_HEIGHT));
  const cols = columnLayout(width, p.maxLanes, p.columns);
  const { textX, authorX, dateX } = cols;
  const graphEnd = cols.graphX + cols.graph;
  const lx = (lane: number) => laneX(lane, cols.laneOffset);
  const font = labelFont(palette);
  const widths = p.labelWidths?.font === font ? p.labelWidths : new TextWidthCache(font);
  const measureLabel = (t: string) => widths.measure(ctx, t);
  const fitLabel = (t: string, max: number) => widths.fit(ctx, t, max);
  const textFont = `12px ${palette.font}`;
  const initialsFont = `700 8px ${palette.font}`;
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
    const row = p.getRow(i);
    const mark = p.highlight?.(i);
    if (!row) {
      missing++;
      if (mark) {
        ctx.fillStyle = palette.primary;
        ctx.globalAlpha = mark === "current" ? 0.22 : 0.09;
        ctx.fillRect(0, y0, width, ROW_HEIGHT);
        ctx.globalAlpha = 1;
      }
      ctx.fillStyle = palette.surface2;
      ctx.fillRect(textX, cy - 4, Math.min(320, authorX - textX - 16), 8);
      continue;
    }

    const laneColor = palette.lanes[row.color % 8]!;
    const nx = lx(Math.min(row.lane, MAX_DRAWN_LANES - 1));
    const pills = rowPills(row);
    const cell = pills.length > 0 ? refCell(pills, cols, measureLabel, fitLabel) : null;

    // Faint lane-colored band from the node to the end of the row (stronger on branch tips).
    ctx.fillStyle = laneColor;
    ctx.globalAlpha = cell ? 0.1 : 0.045;
    ctx.fillRect(Math.min(nx, graphEnd), y0 + 1, width, ROW_HEIGHT - 2);
    if (cell) {
      ctx.globalAlpha = 0.06;
      ctx.fillRect(0, y0 + 1, Math.min(nx, graphEnd), ROW_HEIGHT - 2);
    }
    ctx.globalAlpha = 1;
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

    // Connector from the ref pill to the node (under the graph lines).
    if (cell) {
      const from = Math.min(cell.end, cols.refs) + 1;
      const to = Math.min(nx - AVATAR_RADIUS, graphEnd);
      if (to > from) {
        ctx.strokeStyle = laneColor;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(from, cy);
        ctx.lineTo(to, cy);
        ctx.stroke();
      }
    }

    // Graph lines: upper half then lower half, clipped to the graph column.
    ctx.save();
    ctx.beginPath();
    ctx.rect(cols.graphX, y0, cols.graph, ROW_HEIGHT);
    ctx.clip();
    ctx.lineWidth = 2;
    const s = row.segments;
    for (let k = 0; k < s.length; k += 4) {
      const kind = s[k]!;
      const from = s[k + 1]!;
      const to = s[k + 2]!;
      if (from >= MAX_DRAWN_LANES && to >= MAX_DRAWN_LANES) continue;
      ctx.strokeStyle = palette.lanes[s[k + 3]! % 8]!;
      ctx.beginPath();
      if (kind === 0) curve(ctx, lx(from), y0, lx(to), cy);
      else curve(ctx, lx(from), cy, lx(to), y0 + ROW_HEIGHT);
      ctx.stroke();
    }

    // Node: merge = small hollow dot; otherwise the author's avatar (or initials) in a ring.
    if (row.isMerge) {
      ctx.beginPath();
      ctx.arc(nx, cy, DOT_RADIUS, 0, TAU);
      ctx.fillStyle = palette.bg;
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = laneColor;
      ctx.stroke();
    } else {
      ctx.beginPath();
      ctx.arc(nx, cy, AVATAR_RADIUS, 0, TAU);
      ctx.fillStyle = laneColor;
      ctx.fill();
      const img = p.avatar?.(row.authorEmail) ?? null;
      ctx.beginPath();
      ctx.arc(nx, cy, AVATAR_INNER, 0, TAU);
      if (img) {
        ctx.fillStyle = palette.bg;
        ctx.fill();
        ctx.save();
        ctx.clip();
        ctx.drawImage(
          img,
          nx - AVATAR_INNER,
          cy - AVATAR_INNER,
          AVATAR_INNER * 2,
          AVATAR_INNER * 2,
        );
        ctx.restore();
      } else {
        const badge = initialsBadge(row.authorName, row.authorEmail, palette.lanes);
        ctx.fillStyle = badge.color === laneColor ? palette.bg : badge.color;
        ctx.fill();
        ctx.font = initialsFont;
        ctx.textAlign = "center";
        ctx.fillStyle = badge.color === laneColor ? palette.text : badge.fg;
        ctx.fillText(badge.text, nx, cy + 0.5);
        ctx.textAlign = "start";
      }
    }
    ctx.restore();

    // "Branch / Tag" cell: first pill + "+N".
    if (cell) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(0, y0, cols.refs, ROW_HEIGHT);
      ctx.clip();
      const { pill } = cell;
      const top = cy - PILL_HEIGHT / 2;
      const radius = pill.tag ? 3 : PILL_HEIGHT / 2;
      if (p.dropTarget?.index === i && p.dropTarget.branch === pill.label.text && pill.local) {
        ctx.strokeStyle = palette.primary;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.roundRect(cell.x - 2, top - 2, cell.width + 4, PILL_HEIGHT + 4, radius + 2);
        ctx.stroke();
      }
      ctx.fillStyle = laneColor;
      ctx.beginPath();
      ctx.roundRect(cell.x, top, cell.width, PILL_HEIGHT, radius);
      ctx.fill();
      const fg = contrastText(laneColor);
      ctx.fillStyle = fg;
      ctx.strokeStyle = fg;
      if (pill.head) drawCheck(ctx, cell.x + PILL_PAD, cy);
      if (pill.tag) drawTag(ctx, cell.nameX - ICON_GAP - TAG_W, cy, laneColor);
      ctx.font = font;
      ctx.fillText(cell.name, cell.nameX, cy + 0.5);
      let x = cell.x + cell.width - PILL_PAD;
      if (pill.remote) {
        x -= CLOUD_W;
        drawCloud(ctx, x, cy);
        x -= ICON_GAP;
      }
      if (pill.local) drawLaptop(ctx, x - LAPTOP_W, cy);
      if (cell.more > 0) {
        ctx.fillStyle = palette.surface2;
        ctx.strokeStyle = laneColor;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.roundRect(
          cell.moreX + 0.5,
          top + 0.5,
          cell.moreWidth - 1,
          PILL_HEIGHT - 1,
          PILL_HEIGHT / 2,
        );
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = palette.text;
        ctx.fillText(cell.moreText, cell.moreX + MORE_PAD, cy + 0.5);
      }
      ctx.restore();
    }

    // Subject, clipped before the author column.
    ctx.save();
    ctx.beginPath();
    ctx.rect(textX, y0, Math.max(0, cols.messageEnd - textX), ROW_HEIGHT);
    ctx.clip();
    ctx.font = textFont;
    ctx.fillStyle = row.isMerge ? palette.muted : palette.text;
    ctx.fillText(row.subject, textX, cy + 0.5);
    ctx.restore();

    if (cols.author > 0) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(authorX, y0, cols.author - 12, ROW_HEIGHT);
      ctx.clip();
      ctx.font = textFont;
      ctx.fillStyle = palette.muted;
      ctx.fillText(row.authorName, authorX, cy + 0.5);
      ctx.restore();
    }

    if (cols.date > 0) {
      ctx.font = `11px ${palette.mono}`;
      ctx.fillStyle = palette.muted;
      ctx.fillText(rowDate(row, p.locale), dateX, cy + 0.5);
    }
  }
  return missing;
}
