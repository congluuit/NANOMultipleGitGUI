import type { GraphRow } from "@/ipc";
import { readToken } from "@/theme/theme";

export const ROW_HEIGHT = 28;
export const LANE_WIDTH = 14;
const GRAPH_PAD = 10;
const NODE_RADIUS = 5;
const AUTHOR_WIDTH = 170;
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

/**
 * Author / date column widths for a graph `width` px wide (0 = column hidden): in a narrow
 * window the author column shrinks, then hides, then the date hides, so the message stays.
 */
export function sideColumns(width: number, maxLanes: number): { author: number; date: number } {
  const free = width - graphWidth(maxLanes) - 8 - MIN_MESSAGE_WIDTH;
  if (width <= 0 || free >= AUTHOR_WIDTH + DATE_WIDTH) {
    return { author: AUTHOR_WIDTH, date: DATE_WIDTH };
  }
  if (free >= MIN_AUTHOR_WIDTH + DATE_WIDTH) return { author: free - DATE_WIDTH, date: DATE_WIDTH };
  if (free >= DATE_WIDTH) return { author: 0, date: DATE_WIDTH };
  return { author: 0, date: 0 };
}

export const laneX = (lane: number) => GRAPH_PAD + lane * LANE_WIDTH + LANE_WIDTH / 2;

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

/** Left edge of the message column (labels start there). */
export const textColumnX = (maxLanes: number) => graphWidth(maxLanes) + 8;

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

/** Draws the visible window. Returns how many visible rows were not loaded yet. */
export function drawGraph(p: DrawParams): number {
  const { ctx, width, height, scrollTop, palette } = p;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT));
  const last = Math.min(p.total, Math.ceil((scrollTop + height) / ROW_HEIGHT));
  const gWidth = graphWidth(p.maxLanes);
  const textX = gWidth + 8;
  const cols = sideColumns(width, p.maxLanes);
  const authorX = width - cols.author - cols.date;
  const dateX = width - cols.date;
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

    // Graph lines: upper half then lower half.
    ctx.lineWidth = 2;
    const s = row.segments;
    for (let k = 0; k < s.length; k += 4) {
      const [kind, from, to, color] = [s[k]!, s[k + 1]!, s[k + 2]!, s[k + 3]!];
      if (from >= MAX_DRAWN_LANES && to >= MAX_DRAWN_LANES) continue;
      ctx.strokeStyle = palette.lanes[color % 8]!;
      ctx.beginPath();
      if (kind === 0) curve(ctx, laneX(from), y0, laneX(to), cy);
      else curve(ctx, laneX(from), cy, laneX(to), y0 + ROW_HEIGHT);
      ctx.stroke();
    }

    // Node.
    const nx = laneX(Math.min(row.lane, MAX_DRAWN_LANES - 1));
    const laneColor = palette.lanes[row.color % 8]!;
    ctx.beginPath();
    ctx.arc(nx, cy, NODE_RADIUS, 0, Math.PI * 2);
    ctx.fillStyle = row.isMerge ? palette.bg : laneColor;
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = row.isMerge ? laneColor : palette.bg;
    ctx.stroke();

    // Ref labels + subject, clipped before the author column.
    ctx.save();
    ctx.beginPath();
    ctx.rect(textX, y0, authorX - textX - 12, ROW_HEIGHT);
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
    ctx.font = `12px ${palette.font}`;
    ctx.fillStyle = row.isMerge ? palette.muted : palette.text;
    ctx.fillText(row.subject, x + (x > textX ? 4 : 0), cy + 0.5);
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
