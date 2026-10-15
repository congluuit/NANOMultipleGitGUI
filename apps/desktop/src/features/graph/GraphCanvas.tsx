import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { commands, type GraphRow, type GraphSummary } from "@/ipc";
import { registerDropResolver, startBranchDrag, useDnd } from "@/features/dnd/store";
import type { ResolvedTheme } from "@/theme/theme";
import type { FrameStats } from "./benchmark";
import {
  drawGraph,
  labelAt,
  labelFont,
  parseRefs,
  readPalette,
  refLabelBoxes,
  ROW_HEIGHT,
  textColumnX,
  type Palette,
} from "./draw";
import { nextIndex, scrollTopFor, type ScrollAlign } from "./navigation";
import { RowCache } from "./rowCache";

const PREFETCH_ROWS = 600;

export interface GraphCanvasHandle {
  scroller: HTMLDivElement | null;
  stats: FrameStats;
  scrollToIndex: (index: number, align: ScrollAlign) => void;
  focus: () => void;
}

interface Props {
  repoId: number;
  summary: GraphSummary;
  theme: ResolvedTheme;
  locale: string;
  selectedIndex: number | null;
  /** Row to show at the top when the canvas mounts (keeps the view stable across reloads). */
  initialTopIndex?: number;
  highlight?: (index: number) => "match" | "current" | null;
  onSelect: (index: number | null, row: GraphRow | undefined) => void;
  /** Reports the commit at the top of the viewport after scrolling. */
  onTopRowChange?: (hash: string) => void;
  /** Called once, when every row of the first screen has been painted. */
  onFirstPaint?: () => void;
  /** Right-click on a loaded row (it is selected first). */
  onRowContextMenu?: (index: number, row: GraphRow) => void;
}

export const GraphCanvas = forwardRef<GraphCanvasHandle, Props>(function GraphCanvas(
  {
    repoId,
    summary,
    theme,
    locale,
    selectedIndex,
    initialTopIndex = 0,
    highlight,
    onSelect,
    onTopRowChange,
    onFirstPaint,
    onRowContextMenu,
  },
  ref,
) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const statsRef = useRef<FrameStats>({ drawMs: [] });
  const [cache] = useState(
    () =>
      new RowCache<GraphRow>(
        (offset, limit) => commands.graphRows(repoId, offset, limit),
        summary.total,
      ),
  );
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [hovered, setHovered] = useState<number | null>(null);
  const paletteRef = useRef<Palette | null>(null);
  const frame = useRef(0);
  const firstPaintDone = useRef(false);
  const initialScrollDone = useRef(false);
  const lastTopHash = useRef<string | null>(null);
  // Branch label outlined while another branch is dragged over the graph (step 3.1).
  const [dropTarget, setDropTarget] = useState<{ index: number; branch: string } | null>(null);
  const [overLabel, setOverLabel] = useState(false);
  const dragging = useDnd((s) => s.drag !== null);

  const scrollToIndex = useCallback((index: number, align: ScrollAlign) => {
    const el = scrollerRef.current;
    if (!el) return;
    const top = scrollTopFor(index, align, el.scrollTop, el.clientHeight);
    if (top !== null) el.scrollTop = top;
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      scroller: scrollerRef.current,
      stats: statsRef.current,
      scrollToIndex,
      focus: () => scrollerRef.current?.focus({ preventScroll: true }),
    }),
    [scrollToIndex],
  );

  const draw = useCallback(() => {
    frame.current = 0;
    const canvas = canvasRef.current;
    const scroller = scrollerRef.current;
    if (!canvas || !scroller || size.width === 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    paletteRef.current ??= readPalette();
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const scrollTop = scroller.scrollTop;
    const first = Math.floor(scrollTop / ROW_HEIGHT);
    const last = Math.ceil((scrollTop + size.height) / ROW_HEIGHT);
    cache.ensure(first - PREFETCH_ROWS, last + PREFETCH_ROWS, schedule);

    const t = performance.now();
    const missing = drawGraph({
      ctx,
      width: size.width,
      height: size.height,
      scrollTop,
      total: summary.total,
      maxLanes: summary.maxLanes,
      getRow: (i) => cache.get(i),
      palette: paletteRef.current,
      selected: selectedIndex,
      hovered,
      highlight,
      locale,
      dropTarget: dragging ? dropTarget : null,
    });
    statsRef.current.drawMs.push(performance.now() - t);
    if (statsRef.current.drawMs.length > 5000) statsRef.current.drawMs.shift();

    const topHash = cache.get(first)?.hash;
    if (topHash && topHash !== lastTopHash.current) {
      lastTopHash.current = topHash;
      onTopRowChange?.(topHash);
    }
    if (!missing && !firstPaintDone.current) {
      firstPaintDone.current = true;
      onFirstPaint?.();
    }
  }, [
    cache,
    size,
    summary,
    selectedIndex,
    hovered,
    highlight,
    locale,
    onFirstPaint,
    onTopRowChange,
    dragging,
    dropTarget,
  ]);

  const drawRef = useRef(draw);
  useEffect(() => {
    drawRef.current = draw;
  }, [draw]);

  function schedule() {
    if (!frame.current) frame.current = requestAnimationFrame(() => drawRef.current());
  }

  // Re-read colors when the theme changes.
  useEffect(() => {
    paletteRef.current = null;
    schedule();
  }, [theme]);

  useEffect(() => {
    schedule();
  }, [draw]);

  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setSize({ width: el.clientWidth, height: el.clientHeight });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.width * dpr);
    canvas.height = Math.round(size.height * dpr);
    canvas.style.width = `${size.width}px`;
    canvas.style.height = `${size.height}px`;
    // The spacer only has its final height once the viewport is measured.
    if (size.height > 0 && !initialScrollDone.current) {
      initialScrollDone.current = true;
      if (initialTopIndex > 0) scrollToIndex(initialTopIndex, "top");
    }
  }, [size, initialTopIndex, scrollToIndex]);

  const rowAt = (clientY: number) => {
    const el = scrollerRef.current!;
    const y = clientY - el.getBoundingClientRect().top + el.scrollTop;
    const i = Math.floor(y / ROW_HEIGHT);
    return i >= 0 && i < summary.total ? i : null;
  };

  /**
   * Local branch under a point: its label, or — when `rowFallback` — the only local branch of
   * the row under the pointer (so dropping anywhere on that row works).
   */
  const branchAtPoint = useCallback(
    (clientX: number, clientY: number, rowFallback: boolean) => {
      const el = scrollerRef.current;
      const ctx = canvasRef.current?.getContext("2d");
      if (!el || !ctx) return null;
      const rect = el.getBoundingClientRect();
      const i = Math.floor((clientY - rect.top + el.scrollTop) / ROW_HEIGHT);
      const row = i >= 0 && i < summary.total ? cache.get(i) : undefined;
      if (!row) return null;
      const labels = parseRefs(row.refs).filter(
        (l) => (l.kind === "branch" || l.kind === "head") && l.text !== "HEAD",
      );
      paletteRef.current ??= readPalette();
      ctx.save();
      ctx.font = labelFont(paletteRef.current);
      const boxes = refLabelBoxes(
        parseRefs(row.refs),
        textColumnX(summary.maxLanes),
        (text) => ctx.measureText(text).width,
      );
      ctx.restore();
      const hit = labelAt(boxes, clientX - rect.left);
      const name =
        hit && labels.some((l) => l.text === hit.text)
          ? hit.text
          : rowFallback && labels.length === 1
            ? labels[0]!.text
            : null;
      return name ? { index: i, branch: name } : null;
    },
    [cache, summary],
  );

  // Drop target for branches dragged from the sidebar or from another label.
  useEffect(
    () =>
      registerDropResolver(`graph-${repoId}`, (x, y) => {
        const hit = branchAtPoint(x, y, true);
        setDropTarget(hit);
        return hit?.branch ?? null;
      }),
    [repoId, branchAtPoint],
  );

  const selectIndex = (i: number | null) => onSelect(i, i === null ? undefined : cache.get(i));

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      selectIndex(null);
      return;
    }
    const pageRows = Math.max(1, Math.floor(size.height / ROW_HEIGHT) - 1);
    const next = nextIndex(e.key, selectedIndex, summary.total, pageRows);
    if (next === null) return;
    e.preventDefault();
    scrollToIndex(next, "nearest");
    const row = cache.get(next);
    if (row) {
      onSelect(next, row);
    } else {
      // Not fetched yet (e.g. End on a huge history): select once the page arrives.
      cache.ensure(next, next + 1, () => onSelect(next, cache.get(next)));
    }
  };

  return (
    <div
      ref={scrollerRef}
      tabIndex={0}
      className="relative min-h-0 flex-1 overflow-y-auto focus-visible:outline-none"
      onScroll={schedule}
      data-drop-resolver={`graph-${repoId}`}
      style={{ cursor: overLabel ? "grab" : undefined }}
      onMouseMove={(e) => {
        setHovered(rowAt(e.clientY));
        const over = !dragging && branchAtPoint(e.clientX, e.clientY, false) !== null;
        if (over !== overLabel) setOverLabel(over);
      }}
      onMouseLeave={() => setHovered(null)}
      onMouseDown={(e) => {
        const el = scrollerRef.current!;
        // Ignore presses on the scrollbar.
        if (e.clientX - el.getBoundingClientRect().left > el.clientWidth) return;
        selectIndex(rowAt(e.clientY));
        // Drag a local branch label onto another branch to merge / rebase.
        const label = branchAtPoint(e.clientX, e.clientY, false);
        if (label) startBranchDrag(e, repoId, label.branch);
      }}
      onContextMenu={(e) => {
        const i = rowAt(e.clientY);
        const row = i === null ? undefined : cache.get(i);
        if (i === null || !row || !onRowContextMenu) {
          // No row under the pointer: suppress the webview's own menu as well.
          e.preventDefault();
          return;
        }
        onSelect(i, row);
        onRowContextMenu(i, row);
      }}
      onKeyDown={onKeyDown}
      role="grid"
      aria-rowcount={summary.total}
    >
      <canvas ref={canvasRef} className="sticky top-0 block" />
      <div style={{ height: Math.max(0, summary.total * ROW_HEIGHT - size.height) }} />
    </div>
  );
});
