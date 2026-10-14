import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { ChevronDown, ChevronUp, Gauge, LoaderCircle, Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { repoKeys } from "@/features/repos/queries";
import { WipRow } from "@/features/worktree/WipRow";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { CommitMenuContent } from "@/features/commits/CommitMenu";
import {
  commands,
  errorMessage,
  type GraphBenchmark,
  type GraphRow,
  type GraphSummary,
} from "@/ipc";
import { useGraphStore, useSelection } from "@/stores/graph";
import { useUi } from "@/stores/ui";
import { runScrollBenchmark } from "./benchmark";
import { graphWidth } from "./draw";
import { GraphCanvas, type GraphCanvasHandle } from "./GraphCanvas";
import { cycleMatch } from "./navigation";

const SEARCH_LIMIT = 5000;

interface ViewState {
  summary: GraphSummary;
  /** Row kept at the top when a new version of the history is swapped in. */
  initialTop: number;
}

interface SearchState {
  query: string;
  matches: number[];
  current: number;
}

export function GraphView({ repoId, name }: { repoId: number; name: string }) {
  const { t, i18n } = useTranslation();
  const theme = useUi((s) => s.resolvedTheme);
  const benchOnLoad = useUi((s) => s.benchOnLoad);
  const selection = useSelection(repoId);
  const select = useGraphStore((s) => s.select);
  const jump = useGraphStore((s) => s.jump);
  const canvas = useRef<GraphCanvasHandle>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const openStarted = useRef(0);
  const viewVersion = useRef<number | null>(null);
  const topHash = useRef<string | null>(null);
  const handledJump = useRef(0);
  const [view, setView] = useState<ViewState | null>(null);
  const [firstPaintMs, setFirstPaintMs] = useState<number | null>(null);
  const [bench, setBench] = useState<GraphBenchmark | null>(null);
  const [running, setRunning] = useState(false);
  const [search, setSearch] = useState<SearchState>({ query: "", matches: [], current: -1 });
  const [menuRow, setMenuRow] = useState<GraphRow | null>(null);
  const onRowContextMenu = useCallback((_: number, row: GraphRow) => setMenuRow(row), []);

  useEffect(() => {
    openStarted.current = performance.now();
  }, []);

  const summary = useQuery({
    queryKey: repoKeys.graph(repoId),
    queryFn: () => commands.graphOpen(repoId),
    enabled: isTauri(),
    staleTime: Infinity,
    retry: false,
  });

  // Swap in a new history version only after re-resolving the top row and the selection by
  // hash, so a refresh (new commit, fetch) keeps the view where the user left it.
  useEffect(() => {
    const s = summary.data;
    if (!s) return;
    if (viewVersion.current === s.version) {
      setView((v) => (v ? { ...v, summary: s } : v));
      return;
    }
    const isFirst = viewVersion.current === null;
    let cancelled = false;
    void (async () => {
      const sel = useGraphStore.getState().selection[repoId] ?? null;
      const anchor = isFirst ? null : topHash.current;
      const [top, selIndex] = await Promise.all([
        anchor ? commands.graphFind(repoId, anchor) : Promise.resolve(0),
        sel ? commands.graphFind(repoId, sel.hash) : Promise.resolve(null),
      ]);
      if (cancelled) return;
      viewVersion.current = s.version;
      if (!isFirst) openStarted.current = performance.now();
      setFirstPaintMs(null);
      setView({ summary: s, initialTop: top ?? 0 });
      if (sel) select(repoId, selIndex === null ? null : { hash: sel.hash, index: selIndex });
    })().catch((e: unknown) => console.error("graph refresh failed", e));
    return () => {
      cancelled = true;
    };
  }, [summary.data, repoId, select]);

  /** Selects row `index` (resolving its hash) and scrolls it into view. */
  const goToIndex = useCallback(
    async (index: number) => {
      const [row] = await commands.graphRows(repoId, index, 1);
      if (!row) return;
      select(repoId, { hash: row.hash, index });
      canvas.current?.scrollToIndex(index, "center");
    },
    [repoId, select],
  );

  // Jumps requested from elsewhere (sidebar branch/tag, parent links in commit details).
  useEffect(() => {
    if (!jump || jump.repoId !== repoId || !view || handledJump.current === jump.nonce) return;
    handledJump.current = jump.nonce;
    void commands.graphFind(repoId, jump.oid).then((index) => {
      if (index === null) {
        useUi.getState().showNotice({
          kind: "info",
          message: t("graph.notInHistory", { oid: jump.oid.slice(0, 7) }),
        });
      } else {
        void goToIndex(index);
        canvas.current?.focus();
      }
    });
  }, [jump, repoId, view, goToIndex, t]);

  // Search (debounced). A new query jumps to its first match; when only the history changed
  // (refresh), matches are recomputed but the user's selection stays where it is.
  const query = search.query.trim();
  const version = view?.summary.version;
  const searchedQuery = useRef("");
  useEffect(() => {
    if (!query) searchedQuery.current = "";
    if (!query || version === undefined) return;
    const timer = setTimeout(() => {
      void commands.graphSearch(repoId, query, SEARCH_LIMIT).then((matches) => {
        const newQuery = searchedQuery.current !== query;
        searchedQuery.current = query;
        const selected = useGraphStore.getState().selection[repoId]?.index;
        const current = newQuery
          ? matches.length
            ? 0
            : -1
          : selected === undefined
            ? -1
            : matches.indexOf(selected);
        setSearch((s) => (s.query.trim() === query ? { ...s, matches, current } : s));
        if (newQuery && matches[0] !== undefined) void goToIndex(matches[0]);
      });
    }, 200);
    return () => clearTimeout(timer);
  }, [query, version, repoId, goToIndex]);

  const stepMatch = (backwards: boolean) => {
    const current = cycleMatch(search.current, search.matches.length, backwards);
    const index = search.matches[current];
    setSearch({ ...search, current });
    if (index !== undefined) void goToIndex(index);
  };

  const matchSet = useMemo(() => new Set(search.matches), [search.matches]);
  const currentMatch = search.matches[search.current];
  const highlight = useMemo(() => {
    if (!query || matchSet.size === 0) return undefined;
    return (i: number) => (i === currentMatch ? "current" : matchSet.has(i) ? "match" : null);
  }, [query, matchSet, currentMatch]);

  // Ctrl/Cmd+F focuses the search box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchInput.current?.focus();
        searchInput.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onFirstPaint = useCallback(() => {
    setFirstPaintMs(performance.now() - openStarted.current);
  }, []);
  const onTopRowChange = useCallback((hash: string) => {
    topHash.current = hash;
  }, []);
  const onSelect = useCallback(
    (index: number | null, row: { hash: string } | undefined) =>
      select(repoId, index === null || !row ? null : { hash: row.hash, index }),
    [repoId, select],
  );

  const runBenchmark = useCallback(async () => {
    const handle = canvas.current;
    if (!handle?.scroller || !view) return;
    setRunning(true);
    try {
      const result = await runScrollBenchmark(handle.scroller, handle.stats, {
        totalRows: view.summary.total,
        firstPaintMs: firstPaintMs ?? 0,
      });
      setBench(result);
      await commands.reportBenchmark(result);
    } finally {
      setRunning(false);
    }
  }, [view, firstPaintMs]);

  useEffect(() => {
    if (benchOnLoad && firstPaintMs !== null && !bench && !running) {
      useUi.setState({ benchOnLoad: false });
      void runBenchmark();
    }
  }, [benchOnLoad, firstPaintMs, bench, running, runBenchmark]);

  const s = view?.summary;

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b px-3 text-xs">
        <span className="shrink-0 font-semibold whitespace-nowrap">{name}</span>
        {s && (
          <span className="min-w-0 truncate text-muted-foreground tabular-nums">
            {t("graph.stats", {
              total: s.total.toLocaleString(i18n.language),
              lanes: s.maxLanes,
              ms: s.loadMs,
            })}
            {firstPaintMs !== null &&
              ` · ${t("graph.firstPaint", { ms: Math.round(firstPaintMs) })}`}
          </span>
        )}
        <span className="flex-1" />
        {bench && (
          <span className="shrink-0 rounded bg-success/15 px-2 py-0.5 font-medium whitespace-nowrap text-success tabular-nums">
            {t("graph.benchResult", {
              fps: bench.avgFps,
              p95: bench.p95FrameMs,
              draw: bench.avgDrawMs,
              slow: bench.slowFrames,
            })}
          </span>
        )}
        <div className="flex h-7 w-72 shrink-0 items-center gap-1 rounded-md border bg-surface px-2 focus-within:border-primary">
          <Search className="size-3.5 shrink-0 text-muted-foreground" />
          <input
            ref={searchInput}
            value={search.query}
            disabled={!s}
            placeholder={t("graph.search")}
            aria-label={t("graph.search")}
            onChange={(e) => setSearch({ query: e.target.value, matches: [], current: -1 })}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                stepMatch(e.shiftKey);
              } else if (e.key === "Escape") {
                setSearch({ query: "", matches: [], current: -1 });
                canvas.current?.focus();
              }
            }}
            className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground"
          />
          {query && (
            <>
              <span
                className={
                  search.matches.length
                    ? "shrink-0 text-muted-foreground tabular-nums"
                    : "shrink-0 text-danger"
                }
              >
                {search.matches.length
                  ? t("graph.searchCount", {
                      current: search.current >= 0 ? search.current + 1 : "–",
                      total:
                        search.matches.length >= SEARCH_LIMIT
                          ? `${SEARCH_LIMIT}+`
                          : search.matches.length,
                    })
                  : t("graph.noMatches")}
              </span>
              <button
                type="button"
                aria-label={t("graph.prevMatch")}
                disabled={!search.matches.length}
                onClick={() => stepMatch(true)}
                className="rounded p-0.5 hover:bg-surface-2 disabled:opacity-40"
              >
                <ChevronUp className="size-3.5" />
              </button>
              <button
                type="button"
                aria-label={t("graph.nextMatch")}
                disabled={!search.matches.length}
                onClick={() => stepMatch(false)}
                className="rounded p-0.5 hover:bg-surface-2 disabled:opacity-40"
              >
                <ChevronDown className="size-3.5" />
              </button>
              <button
                type="button"
                aria-label={t("graph.clearSearch")}
                onClick={() => setSearch({ query: "", matches: [], current: -1 })}
                className="rounded p-0.5 hover:bg-surface-2"
              >
                <X className="size-3.5" />
              </button>
            </>
          )}
        </div>
        <Button
          variant="outline"
          size="xs"
          disabled={!s || running}
          onClick={() => void runBenchmark()}
          title={t("graph.benchmark")}
        >
          {running ? <LoaderCircle className="animate-spin" /> : <Gauge />}
        </Button>
      </div>
      <div
        className="flex h-7 shrink-0 items-center border-b text-[11px] font-semibold text-muted-foreground uppercase"
        style={{ paddingLeft: 10 }}
      >
        <span
          className="shrink-0 truncate whitespace-nowrap"
          style={{ width: graphWidth(s?.maxLanes ?? 1) - 2 }}
        >
          {t("graph.colGraph")}
        </span>
        <span className="flex-1">{t("graph.colMessage")}</span>
        <span className="w-[170px]">{t("graph.colAuthor")}</span>
        <span className="w-[130px]">{t("graph.colDate")}</span>
      </div>
      <WipRow repoId={repoId} />
      {!view && summary.isPending && (
        <div className="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          {t("graph.loading")}
        </div>
      )}
      {summary.isError && (
        <div className="flex flex-1 items-center justify-center p-6 text-center text-danger">
          {t("graph.error", { message: errorMessage(summary.error) })}
        </div>
      )}
      {s && s.total === 0 && (
        <div className="flex flex-1 items-center justify-center text-xs text-muted-foreground">
          {t("graph.empty")}
        </div>
      )}
      {view && s && s.total > 0 && (
        <ContextMenu onOpenChange={(open) => !open && setMenuRow(null)}>
          <ContextMenuTrigger asChild>
            <div className="flex min-h-0 flex-1 flex-col">
              <GraphCanvas
                key={`${repoId}-${s.version}`}
                ref={canvas}
                repoId={repoId}
                summary={s}
                theme={theme}
                locale={i18n.language}
                selectedIndex={selection?.index ?? null}
                initialTopIndex={view.initialTop}
                highlight={highlight}
                onSelect={onSelect}
                onTopRowChange={onTopRowChange}
                onFirstPaint={onFirstPaint}
                onRowContextMenu={onRowContextMenu}
              />
            </div>
          </ContextMenuTrigger>
          {menuRow && <CommitMenuContent repoId={repoId} row={menuRow} />}
        </ContextMenu>
      )}
    </div>
  );
}
