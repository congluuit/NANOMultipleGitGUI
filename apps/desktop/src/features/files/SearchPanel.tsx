import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import {
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  FolderGit2,
  LoaderCircle,
  RefreshCw,
  Regex,
  Square,
  WholeWord,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { openInEditor } from "@/features/editor/open";
import { commands, errorMessage, type SearchMatch } from "@/ipc";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";
import { useSearchState, useSidebarView, type SearchScope } from "./store";
import { groupByFile, toggled } from "./search";
import { FileGroups } from "./SearchResults";
import {
  cancelWorkspaceSearch,
  groupWorkspaceResults,
  runWorkspaceSearch,
  useWorkspaceSearch,
  workspaceTotals,
  WORKSPACE_LIMIT,
} from "./workspaceSearch";

const LIMIT = 2000;
/** Options line, errors and the summary above the results. */
const HEADER = "space-y-1 px-2 pt-1 pb-2";

/**
 * Search across the files (`git grep`) of the active repository, or of every open repository;
 * a click on a line opens the file at that line.
 */
export function SearchPanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const query = useSearchState((s) => s.query);
  const setQuery = useSearchState((s) => s.setQuery);
  const options = useSearchState((s) => s.options);
  const setOption = useSearchState((s) => s.setOption);
  const scope = useSearchState((s) => s.scope);
  const setScope = useSearchState((s) => s.setScope);
  const [debounced, setDebounced] = useState(query);
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

  const all = scope === "all";
  const placeholder = all ? t("search.all.placeholder") : t("search.placeholder");

  return (
    <div className="flex min-h-0 flex-col">
      <div className="space-y-1 px-2">
        <ScopeSwitch scope={scope} onChange={setScope} />
        <div className="flex items-center gap-0.5 rounded-md border bg-background pr-1 focus-within:border-primary">
          <input
            ref={input}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") setDebounced(query);
              if (e.key === "Escape" && all) cancelWorkspaceSearch();
            }}
            placeholder={placeholder}
            aria-label={placeholder}
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
      </div>
      {all ? (
        <WorkspaceResults query={debounced} />
      ) : (
        <RepoResults repoId={repoId} query={debounced} />
      )}
    </div>
  );
}

/** "This repository" / "All open repositories". */
function ScopeSwitch({
  scope,
  onChange,
}: {
  scope: SearchScope;
  onChange: (scope: SearchScope) => void;
}) {
  const { t } = useTranslation();
  const items: { id: SearchScope; label: string }[] = [
    { id: "repo", label: t("search.scope.repo") },
    { id: "all", label: t("search.scope.all") },
  ];
  return (
    <div
      role="radiogroup"
      aria-label={t("search.scope.label")}
      className="flex h-6 items-center gap-0.5 rounded-md bg-surface-2 p-0.5"
    >
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="radio"
          aria-checked={scope === item.id}
          onClick={() => onChange(item.id)}
          className={cn(
            "h-5 min-w-0 flex-1 truncate rounded px-1 text-[11px] text-muted-foreground hover:text-foreground",
            scope === item.id && "bg-background font-medium text-foreground shadow-xs",
          )}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}

/** The "untracked files" checkbox and the busy / refresh button. */
function SearchBar({ busy, children }: { busy: boolean; children?: ReactNode }) {
  const { t } = useTranslation();
  const options = useSearchState((s) => s.options);
  const setOption = useSearchState((s) => s.setOption);
  return (
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
      {busy && <LoaderCircle className="size-3.5 animate-spin" />}
      {children}
    </div>
  );
}

function RefreshButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      title={t("search.refresh")}
      aria-label={t("search.refresh")}
      onClick={onClick}
      className="rounded p-0.5 hover:bg-surface-2 hover:text-foreground"
    >
      <RefreshCw className="size-3.5" />
    </button>
  );
}

/** Results of the active repository. */
function RepoResults({ repoId, query }: { repoId: number; query: string }) {
  const { t } = useTranslation();
  const options = useSearchState((s) => s.options);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());

  const search = useQuery({
    queryKey: ["search", repoId, query, options],
    queryFn: () => commands.repoSearch(repoId, query, options, LIMIT),
    enabled: isTauri() && query.trim().length > 0,
    staleTime: 30_000,
  });

  const groups = useMemo(() => groupByFile(search.data?.matches ?? []), [search.data]);
  const total = search.data?.matches.length ?? 0;

  return (
    <>
      <div className={HEADER}>
        <SearchBar busy={search.isFetching}>
          {!search.isFetching && query.trim() && (
            <RefreshButton onClick={() => void search.refetch()} />
          )}
        </SearchBar>
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
        <FileGroups
          groups={groups}
          collapsed={collapsed}
          onToggle={(key) => setCollapsed((prev) => toggled(prev, key))}
          onOpen={(m) => openInEditor(repoId, m.path, { line: m.line, column: m.column })}
          query={query}
          options={options}
        />
      </ul>
    </>
  );
}

/** Switches to the repository's tab and opens the file at the line. */
function openMatch(repoId: number, m: SearchMatch) {
  const ws = useWorkspace.getState();
  if (ws.activeId !== repoId) ws.setActive(repoId);
  openInEditor(repoId, m.path, { line: m.line, column: m.column });
}

/** Results of every open repository, grouped by repository then by file. */
function WorkspaceResults({ query }: { query: string }) {
  const { t } = useTranslation();
  const options = useSearchState((s) => s.options);
  const tabs = useWorkspace((s) => s.tabs);
  const state = useWorkspaceSearch();
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());

  // A new query or new options start a new search (cancelling the running one).
  useEffect(() => {
    if (!isTauri()) return;
    runWorkspaceSearch(query, options, useWorkspace.getState().tabs.length);
  }, [query, options]);

  const groups = useMemo(() => groupWorkspaceResults(state.results, tabs), [state.results, tabs]);
  const totals = workspaceTotals(groups);
  const running = state.status === "running";
  const toggle = (key: string) => setCollapsed((prev) => toggled(prev, key));
  const shownQuery = state.query;
  const shownOptions = state.options ?? options;

  return (
    <>
      <div className={HEADER}>
        <SearchBar busy={running}>
          {running ? (
            <button
              type="button"
              title={t("search.all.stop")}
              aria-label={t("search.all.stop")}
              onClick={cancelWorkspaceSearch}
              className="rounded p-0.5 hover:bg-surface-2 hover:text-foreground"
            >
              <Square className="size-3.5" />
            </button>
          ) : (
            query.trim() && (
              <RefreshButton
                onClick={() => runWorkspaceSearch(query, options, tabs.length, { force: true })}
              />
            )
          )}
        </SearchBar>
        {state.status === "error" && state.error && (
          <p className="text-[11px] break-words text-danger">
            {t("search.error", { message: state.error })}
          </p>
        )}
        {state.status !== "idle" && state.status !== "error" && (
          <p className="text-[11px] text-muted-foreground" role="status">
            {running
              ? t("search.all.progress", { done: state.results.length, count: state.repoCount })
              : totals.matches === 0
                ? t("search.all.none")
                : t("search.all.summary", {
                    count: totals.matches,
                    files: totals.files,
                    repos: totals.repos,
                  })}
            {state.status === "cancelled" && ` ${t("search.all.stopped")}`}
            {state.summary?.truncated && ` ${t("search.truncated", { limit: WORKSPACE_LIMIT })}`}
            {!!state.summary?.skipped &&
              ` ${t("search.all.skipped", { count: state.summary.skipped })}`}
          </p>
        )}
      </div>
      <ul className="pb-2" aria-label={t("search.results")}>
        {groups.map((g) => {
          const repoKey = `repo:${g.repoId}`;
          const open = !collapsed.has(repoKey);
          const Chevron = open ? ChevronDown : ChevronRight;
          const failed = g.missing || g.error !== null;
          return (
            <li key={g.repoId}>
              <button
                type="button"
                aria-expanded={failed ? undefined : open}
                onClick={() => !failed && toggle(repoKey)}
                className="flex h-6 w-full items-center gap-1 px-2 text-left text-xs hover:bg-surface-2"
              >
                {failed ? (
                  <CircleAlert className="size-3.5 shrink-0 text-danger" />
                ) : (
                  <Chevron className="size-3.5 shrink-0 text-muted-foreground" />
                )}
                <FolderGit2 className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 truncate font-semibold">{g.name}</span>
                {failed ? (
                  <span
                    className="min-w-0 flex-1 truncate text-[11px] text-danger"
                    title={g.error ?? undefined}
                  >
                    {g.missing ? t("search.all.missing") : g.error}
                  </span>
                ) : (
                  <>
                    <span className="flex-1" />
                    <span
                      className="shrink-0 rounded bg-primary/15 px-1 text-[10px] text-primary"
                      title={g.truncated ? t("search.all.repoTruncated") : undefined}
                    >
                      {g.matches.length}
                      {g.truncated && "+"}
                    </span>
                  </>
                )}
              </button>
              {open && !failed && (
                <ul>
                  <FileGroups
                    groups={g.files}
                    collapsed={collapsed}
                    onToggle={toggle}
                    onOpen={(m) => openMatch(g.repoId, m)}
                    query={shownQuery}
                    options={shownOptions}
                    keyPrefix={`${g.repoId}:`}
                    nested
                  />
                </ul>
              )}
            </li>
          );
        })}
      </ul>
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
