import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { commands, type GraphRow, type GraphSummary } from "@/ipc";
import { useAccounts, useIdentities } from "@/features/accounts/queries";
import { registerDropResolver, startBranchDrag, useDnd } from "@/features/dnd/store";
import { useSettings } from "@/stores/settings";
import type { ResolvedTheme } from "@/theme/theme";
import { accountAvatarMap, AvatarCache } from "./avatars";
import type { FrameStats } from "./benchmark";
import { useGraphColumns } from "./columns";
import {
  cellAt,
  cellTooltip,
  columnLayout,
  drawGraph,
  labelFont,
  parseRefs,
  readPalette,
  refAtPoint,
  ROW_HEIGHT,
  TextWidthCache,
  type Palette,
  type RefLabel,
} from "./draw";
import { nextIndex, scrollTopFor, type ScrollAlign } from "./navigation";
import { RowCache } from "./rowCache";

const PREFETCH_ROWS = 600;
/** Hover time before a cut-off cell shows its full text. */
const TOOLTIP_DELAY_MS = 450;

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
  /**
   * Right-click on a loaded row (it is selected first). `ref` = the ref label under the pointer
   * in the "Branch / Tag" column (the local branch of a merged local + remote pill), if any.
   */
  onRowContextMenu?: (index: number, row: GraphRow, ref: RefLabel | null) => void;
  /** Width of the drawing area (without the scrollbar), for the column header. */
  onWidthChange?: (width: number) => void;
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
    onWidthChange,
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
  const columns = useGraphColumns((s) => s.widths);
  const [tip, setTip] = useState<{ x: number; y: number; text: string } | null>(null);
  const tipTimer = useRef<number | undefined>(undefined);
  const tipKey = useRef("");
  // Author avatars on the commit nodes (loaded once per URL, redraw when one arrives).
  const avatarsOn = useSettings((s) => s.authorAvatars);
  const accounts = useAccounts().data;
  const identities = useIdentities(null).data;
  const [avatars] = useState(() => new AvatarCache({ onLoad: () => schedule() }));
  const avatarOf = useCallback((email: string) => avatars.get(email), [avatars]);
  const labelWidthsRef = useRef<TextWidthCache | null>(null);

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
    const labelWidths = labelWidthsFor(paletteRef.current);

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
      columns,
      avatar: avatarOf,
      labelWidths,
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
    columns,
    avatarOf,
  ]);

  const drawRef = useRef(draw);
  useEffect(() => {
    drawRef.current = draw;
  }, [draw]);

  function schedule() {
    if (!frame.current) frame.current = requestAnimationFrame(() => drawRef.current());
  }

  /** Label-font measurements, kept across frames while the font stays the same. */
  function labelWidthsFor(palette: Palette) {
    const font = labelFont(palette);
    if (labelWidthsRef.current?.font !== font) labelWidthsRef.current = new TextWidthCache(font);
    return labelWidthsRef.current;
  }

  // Re-read colors when the theme changes.
  useEffect(() => {
    paletteRef.current = null;
    schedule();
  }, [theme]);

  useEffect(() => {
    avatars.setEnabled(avatarsOn);
    schedule();
  }, [avatars, avatarsOn]);

  useEffect(() => {
    avatars.setAccountAvatars(accountAvatarMap(accounts, identities));
    schedule();
  }, [avatars, accounts, identities]);

  useEffect(() => {
    schedule();
  }, [draw]);

  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setSize({ width: el.clientWidth, height: el.clientHeight });
      onWidthChange?.(el.clientWidth);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [onWidthChange]);

  useEffect(() => () => window.clearTimeout(tipTimer.current), []);

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

  /** Ref label in the "Branch / Tag" column under a point (any kind), with its row. */
  const refAt = useCallback(
    (clientX: number, clientY: number) => {
      const el = scrollerRef.current;
      const ctx = canvasRef.current?.getContext("2d");
      if (!el || !ctx) return null;
      const rect = el.getBoundingClientRect();
      const i = Math.floor((clientY - rect.top + el.scrollTop) / ROW_HEIGHT);
      const row = i >= 0 && i < summary.total ? cache.get(i) : undefined;
      if (!row) return null;
      paletteRef.current ??= readPalette();
      const widths = labelWidthsFor(paletteRef.current);
      ctx.save();
      const label = refAtPoint(
        row,
        columnLayout(el.clientWidth, summary.maxLanes, columns),
        clientX - rect.left,
        (text) => widths.measure(ctx, text),
      );
      ctx.restore();
      return { index: i, row, label };
    },
    [cache, summary, columns],
  );

  /**
   * Local branch under a point: its pill, or — when `rowFallback` — the only local branch of
   * the row under the pointer (so dropping anywhere on that row works).
   */
  const branchAtPoint = useCallback(
    (clientX: number, clientY: number, rowFallback: boolean) => {
      const hit = refAt(clientX, clientY);
      if (!hit) return null;
      const locals = parseRefs(hit.row.refs).filter(
        (l) => (l.kind === "branch" || l.kind === "head") && l.text !== "HEAD",
      );
      const label = hit.label;
      const name =
        label && locals.some((l) => l.text === label.text && l.kind === label.kind)
          ? label.text
          : rowFallback && locals.length === 1
            ? locals[0]!.text
            : null;
      return name ? { index: hit.index, branch: name } : null;
    },
    [refAt],
  );

  /** Full text of a cut-off cell under the pointer, shown after a short hover. */
  const updateTooltip = (clientX: number, clientY: number) => {
    const el = scrollerRef.current;
    const ctx = canvasRef.current?.getContext("2d");
    const i = rowAt(clientY);
    const row = i === null ? undefined : cache.get(i);
    if (!el || !ctx || !row || dragging) {
      hideTooltip();
      return;
    }
    const x = clientX - el.getBoundingClientRect().left;
    const cols = columnLayout(el.clientWidth, summary.maxLanes, columns);
    const cell = cellAt(cols, x);
    const key = `${row.hash}:${cell}`;
    if (key === tipKey.current) return;
    hideTooltip();
    tipKey.current = key;
    if (!cell || cell === "graph") return;
    paletteRef.current ??= readPalette();
    const palette = paletteRef.current;
    const measureWith = (font: string) => (text: string) => {
      ctx.font = font;
      return ctx.measureText(text).width;
    };
    ctx.save();
    const text = cellTooltip(
      row,
      cell,
      cols,
      {
        label: measureWith(labelFont(palette)),
        text: measureWith(`12px ${palette.font}`),
        date: measureWith(`11px ${palette.mono}`),
      },
      locale,
    );
    ctx.restore();
    if (!text) return;
    tipTimer.current = window.setTimeout(
      () => setTip({ x: clientX, y: clientY, text }),
      TOOLTIP_DELAY_MS,
    );
  };

  function hideTooltip() {
    window.clearTimeout(tipTimer.current);
    tipKey.current = "";
    setTip(null);
  }

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
        updateTooltip(e.clientX, e.clientY);
      }}
      onMouseLeave={() => {
        setHovered(null);
        hideTooltip();
      }}
      onWheel={hideTooltip}
      onMouseDown={(e) => {
        const el = scrollerRef.current!;
        // Ignore presses on the scrollbar.
        if (e.clientX - el.getBoundingClientRect().left > el.clientWidth) return;
        hideTooltip();
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
        onRowContextMenu(i, row, refAt(e.clientX, e.clientY)?.label ?? null);
      }}
      onKeyDown={onKeyDown}
      role="grid"
      aria-rowcount={summary.total}
    >
      <canvas ref={canvasRef} className="sticky top-0 block" />
      <div style={{ height: Math.max(0, summary.total * ROW_HEIGHT - size.height) }} />
      {tip &&
        createPortal(
          <div
            role="tooltip"
            className="pointer-events-none fixed z-50 max-w-[min(560px,80vw)] rounded-md bg-foreground px-3 py-1.5 text-xs break-words whitespace-pre-line text-background shadow-md"
            style={{
              left: tip.x > window.innerWidth * 0.6 ? tip.x - 12 : tip.x + 12,
              top: tip.y + 18,
              transform: tip.x > window.innerWidth * 0.6 ? "translateX(-100%)" : undefined,
            }}
          >
            {tip.text}
          </div>,
          document.body,
        )}
    </div>
  );
});
