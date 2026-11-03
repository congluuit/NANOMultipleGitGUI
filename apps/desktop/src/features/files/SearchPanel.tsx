import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import {
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  FileText,
  LoaderCircle,
  RefreshCw,
  Regex,
  WholeWord,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { openInEditor } from "@/features/editor/open";
import { commands, errorMessage, type SearchMatch, type SearchOptions } from "@/ipc";
import { cn } from "@/lib/utils";
import { useSearchState, useSidebarView } from "./store";
import { baseName, parentOf } from "./tree";
import { matchLength } from "./search";

const LIMIT = 2000;

/** Search across the repository's files (`git grep`); a click opens the file at the line. */
export function SearchPanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const query = useSearchState((s) => s.query);
  const setQuery = useSearchState((s) => s.setQuery);
  const options = useSearchState((s) => s.options);
  const setOption = useSearchState((s) => s.setOption);
  const [debounced, setDebounced] = useState(query);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(query), 300);
    return () => window.clearTimeout(id);
  }, [query]);

  const focusTick = useSidebarView((s) => s.focusTick);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focusTick]);

  const search = useQuery({
    queryKey: ["search", repoId, debounced, options],
    queryFn: () => commands.repoSearch(repoId, debounced, options, LIMIT),
    enabled: isTauri() && debounced.trim().length > 0,
    staleTime: 30_000,
  });

  const groups = useMemo(() => {
    const map = new Map<string, SearchMatch[]>();
    for (const m of search.data?.matches ?? []) {
      const list = map.get(m.path);
      if (list) list.push(m);
      else map.set(m.path, [m]);
    }
    return [...map.entries()];
  }, [search.data]);

  const total = search.data?.matches.length ?? 0;

  return (
    <div className="flex min-h-0 flex-col">
      <div className="space-y-1 px-2 pb-2">
        <div className="flex items-center gap-0.5 rounded-md border bg-background pr-1 focus-within:border-primary">
          <input
            ref={input}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") setDebounced(query);
            }}
            placeholder={t("search.placeholder")}
            aria-label={t("search.placeholder")}
            className="h-7 min-w-0 flex-1 bg-transparent px-2 text-xs placeholder:text-muted-foreground focus:outline-none"
          />
          <OptionToggle
            label={t("search.caseSensitive")}
            on={options.caseSensitive}
            onClick={() => setOption("caseSensitive", !options.caseSensitive)}
          >
            <CaseSensitive />
          </OptionToggle>
          <OptionToggle
            label={t("search.wholeWord")}
            on={options.wholeWord}
            onClick={() => setOption("wholeWord", !options.wholeWord)}
          >
            <WholeWord />
          </OptionToggle>
          <OptionToggle
            label={t("search.regex")}
            on={options.regex}
            onClick={() => setOption("regex", !options.regex)}
          >
            <Regex />
          </OptionToggle>
        </div>
        <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={options.untracked}
              onChange={(e) => setOption("untracked", e.target.checked)}
              className="accent-primary"
            />
            {t("search.untracked")}
          </label>
          <span className="flex-1" />
          {search.isFetching ? (
            <LoaderCircle className="size-3.5 animate-spin" />
          ) : (
            debounced.trim() && (
              <button
                type="button"
                title={t("search.refresh")}
                aria-label={t("search.refresh")}
                onClick={() => void search.refetch()}
                className="rounded p-0.5 hover:bg-surface-2 hover:text-foreground"
              >
                <RefreshCw className="size-3.5" />
              </button>
            )
          )}
        </div>
        {search.isError && (
          <p className="text-[11px] break-words text-danger">
            {t("search.error", { message: errorMessage(search.error) })}
          </p>
        )}
        {search.data && (
          <p className="text-[11px] text-muted-foreground">
            {total === 0
              ? t("search.none")
              : t("search.summary", { count: total, files: groups.length })}
            {search.data.truncated && ` ${t("search.truncated", { limit: LIMIT })}`}
          </p>
        )}
      </div>
      <ul className="pb-2" aria-label={t("search.results")}>
        {groups.map(([path, matches]) => {
          const open = !collapsed.has(path);
          const Chevron = open ? ChevronDown : ChevronRight;
          return (
            <li key={path}>
              <button
                type="button"
                title={path}
                onClick={() =>
                  setCollapsed((prev) => {
                    const next = new Set(prev);
                    if (!next.delete(path)) next.add(path);
                    return next;
                  })
                }
                className="flex h-6 w-full items-center gap-1 px-2 text-left text-xs hover:bg-surface-2"
              >
                <Chevron className="size-3.5 shrink-0 text-muted-foreground" />
                <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="shrink-0 font-medium">{baseName(path)}</span>
                <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
                  {parentOf(path)}
                </span>
                <span className="shrink-0 rounded bg-surface-2 px-1 text-[10px]">
                  {matches.length}
                </span>
              </button>
              {open && (
                <ul>
                  {matches.map((m) => (
                    <li key={`${m.line}:${m.column}`}>
                      <button
                        type="button"
                        onClick={() =>
                          openInEditor(repoId, m.path, { line: m.line, column: m.column })
                        }
                        className="flex w-full items-baseline gap-2 py-0.5 pr-2 pl-8 text-left text-xs hover:bg-surface-2"
                      >
                        <span className="w-8 shrink-0 text-right font-mono text-[10px] text-muted-foreground tabular-nums">
                          {m.line}
                        </span>
                        <span className="min-w-0 flex-1 truncate font-mono text-[11px]">
                          <Highlighted match={m} query={debounced} options={options} />
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function Highlighted({
  match,
  query,
  options,
}: {
  match: SearchMatch;
  query: string;
  options: SearchOptions;
}) {
  const start = Math.max(0, match.column - 1 - match.textStart);
  const len = matchLength(match.text, start, query, options);
  const text = match.text;
  // Long lines: start a little before the match so it stays visible.
  const from = Math.max(0, start - 12);
  return (
    <>
      {from > 0 && "..."}
      {text.slice(from, start).trimStart()}
      <mark className="rounded-sm bg-warning/30 text-foreground">
        {text.slice(start, start + len)}
      </mark>
      {text.slice(start + len)}
    </>
  );
}

function OptionToggle({
  label,
  on,
  onClick,
  children,
}: {
  label: string;
  on: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={on}
      onClick={onClick}
      className={cn(
        "flex size-5 items-center justify-center rounded text-muted-foreground hover:text-foreground [&_svg]:size-3.5",
        on && "bg-primary/15 text-primary",
      )}
    >
      {children}
    </button>
  );
}
