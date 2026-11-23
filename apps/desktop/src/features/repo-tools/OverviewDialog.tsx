import { useEffect, useRef, useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpFromLine,
  ChevronDown,
  ChevronRight,
  CloudDownload,
  ArrowDownToLine,
  FolderGit2,
  GitCommitHorizontal,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { accountKeys } from "@/features/accounts/queries";
import { accountLabel } from "@/features/accounts/switcher/model";
import { commands, errorMessage, type RepoOverview } from "@/ipc";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";
import {
  FILTERS,
  GROUP_MODES,
  canPushRow,
  checkState,
  filterCounts,
  groupRows,
  matchesFilter,
  setMany,
  targetRows,
  type AccountTag,
  type CheckState,
  type GroupMode,
  type OverviewFilter,
} from "./bulk";
import { BulkCommitPanel, BulkPushPanel } from "./BulkPanels";
import { useBulkRun } from "./bulkRun";
import { canFetch, canPull, fetchAllRepos, overviewTotals, pullAllRepos } from "./overview";
import { useWorkspaceOverview } from "./queries";

function StateCell({ row }: { row: RepoOverview }) {
  const { t } = useTranslation();
  if (row.missing) return <span className="text-danger">{t("overview.missing")}</span>;
  if (row.error) {
    return (
      <span className="text-danger" title={row.error}>
        {t("overview.unreadable")}
      </span>
    );
  }
  const parts: { text: string; className: string }[] = [];
  if (row.operation) {
    parts.push({
      text: t("ops.inProgress", { op: t(`ops.kind.${row.operation}`) }),
      className: "text-warning",
    });
  }
  if (row.conflicts > 0) {
    parts.push({
      text: t("overview.conflicts", { count: row.conflicts }),
      className: "text-danger",
    });
  }
  if (row.changes > 0) {
    parts.push({ text: t("overview.changes", { count: row.changes }), className: "text-warning" });
  }
  if (row.stashes > 0) {
    parts.push({
      text: t("overview.stashes", { count: row.stashes }),
      className: "text-muted-foreground",
    });
  }
  if (parts.length === 0) {
    return <span className="text-muted-foreground">{t("overview.clean")}</span>;
  }
  return (
    <span className="flex flex-wrap gap-x-2">
      {parts.map((p) => (
        <span key={p.text} className={p.className}>
          {p.text}
        </span>
      ))}
    </span>
  );
}

function SyncCell({ row }: { row: RepoOverview }) {
  const { t } = useTranslation();
  if (row.missing || row.error) return null;
  if (!row.upstream) {
    return <span className="text-muted-foreground">{t("overview.noUpstream")}</span>;
  }
  if (row.ahead === 0 && row.behind === 0) {
    return <span className="text-muted-foreground">{t("overview.inSync")}</span>;
  }
  return (
    <span className="flex items-center gap-2 font-mono tabular-nums">
      {row.behind > 0 && (
        <span
          className="flex items-center gap-0.5 text-info"
          title={t("overview.behindTip", { count: row.behind })}
        >
          <ArrowDown className="size-3" />
          {row.behind}
        </span>
      )}
      {row.ahead > 0 && (
        <span
          className="flex items-center gap-0.5 text-success"
          title={t("overview.aheadTip", { count: row.ahead })}
        >
          <ArrowUp className="size-3" />
          {row.ahead}
        </span>
      )}
    </span>
  );
}

const GROUP_KEY = "nanomgg.overview.group";

function loadGroupMode(): GroupMode {
  try {
    const v = localStorage.getItem(GROUP_KEY);
    return GROUP_MODES.includes(v as GroupMode) ? (v as GroupMode) : "none";
  } catch {
    return "none";
  }
}

function saveGroupMode(mode: GroupMode) {
  try {
    localStorage.setItem(GROUP_KEY, mode);
  } catch {
    // Storage off: the choice is simply not remembered.
  }
}

/** Account bound to each repository (only read while grouping by account). */
function useAccountTags(rows: RepoOverview[], enabled: boolean) {
  const results = useQueries({
    queries: rows.map((r) => ({
      queryKey: accountKeys.repoContext(r.repoId),
      queryFn: () => commands.repoAccountContext(r.repoId),
      enabled: enabled && isTauri() && !r.missing,
    })),
  });
  const tags = new Map<number, AccountTag>();
  rows.forEach((r, i) => {
    const account = results[i]?.data?.account;
    if (account) {
      tags.set(r.repoId, { id: account.id, label: accountLabel(account), color: account.color });
    }
  });
  return (repoId: number) => tags.get(repoId) ?? null;
}

/** A checkbox that can also show "some selected". */
function TriCheckbox({
  state,
  label,
  onChange,
}: {
  state: CheckState;
  label: string;
  onChange: (on: boolean) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = state === "some";
  }, [state]);
  return (
    <input
      ref={ref}
      type="checkbox"
      aria-label={label}
      checked={state === "all"}
      onChange={() => onChange(state !== "all")}
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => e.stopPropagation()}
      className="accent-primary"
    />
  );
}

type View = "list" | "push" | "commit";

/**
 * Every open repository at a glance: filter, group, select, then Fetch / Pull / Push / Commit
 * the selection (or every repository shown when nothing is selected).
 */
export function OverviewDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const overview = useWorkspaceOverview(true);
  const rows = overview.data ?? [];
  const activeId = useWorkspace((s) => s.activeId);
  const lastRun = useBulkRun((s) => s.run);
  const [view, setView] = useState<View>(lastRun?.kind ?? "list");
  const [busy, setBusy] = useState<"fetch" | "pull" | null>(null);
  const [filter, setFilter] = useState<OverviewFilter>("all");
  const [groupMode, setGroupMode] = useState<GroupMode>(loadGroupMode);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());
  // Rows the push / commit view was opened for (kept while the list refreshes).
  const [actionRows, setActionRows] = useState<RepoOverview[]>([]);
  const accountOf = useAccountTags(rows, groupMode === "account");

  const totals = overviewTotals(rows);
  const counts = filterCounts(rows);
  const visible = rows.filter((r) => matchesFilter(r, filter));
  const groups = groupRows(visible, groupMode, accountOf, t("overview.group.unbound"));
  const shownIds = groups.flatMap((g) => g.rows.map((r) => r.repoId));
  // Selected repositories that are still open.
  const selectedCount = rows.filter((r) => selected.has(r.repoId)).length;
  const targets = targetRows(rows, visible, selected);
  const fetchable = targets.filter(canFetch).length;
  const pullable = targets.filter(canPull).length;
  const pushable = targets.filter(canPushRow);
  // Reopened while a run is shown: list what it works on.
  const panelCount = lastRun && view === lastRun.kind ? lastRun.items.length : actionRows.length;

  const run = async (kind: "fetch" | "pull") => {
    setBusy(kind);
    try {
      await (kind === "fetch" ? fetchAllRepos(targets) : pullAllRepos(targets));
    } finally {
      setBusy(null);
    }
  };
  const go = (repoId: number) => {
    useWorkspace.getState().setActive(repoId);
    onClose();
  };
  const changeGroup = (mode: GroupMode) => {
    setGroupMode(mode);
    saveGroupMode(mode);
    setCollapsed(new Set());
  };
  const toggleGroup = (key: string) =>
    setCollapsed((c) => {
      const next = new Set(c);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  const select = (ids: number[], on: boolean) => setSelected((s) => setMany(s, ids, on));
  const openAction = (next: "push" | "commit") => {
    setActionRows(next === "push" ? pushable : targets);
    setView(next);
  };

  const title =
    view === "push"
      ? t("overview.bulk.push.title")
      : view === "commit"
        ? t("overview.bulk.commit.title")
        : t("overview.title");

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {view !== "list"
              ? t("overview.bulk.repoCount", { count: panelCount })
              : rows.length > 0
                ? t("overview.summary", {
                    count: rows.length,
                    behind: totals.behind,
                    ahead: totals.ahead,
                    dirty: totals.dirty,
                  })
                : t("overview.description")}
          </DialogDescription>
        </DialogHeader>
        {view === "push" && (
          <BulkPushPanel rows={actionRows} onBack={() => setView("list")} onOpen={go} />
        )}
        {view === "commit" && (
          <BulkCommitPanel rows={actionRows} onBack={() => setView("list")} onOpen={go} />
        )}
        {view === "list" && (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                size="sm"
                disabled={busy !== null || fetchable === 0}
                onClick={() => void run("fetch")}
              >
                {busy === "fetch" ? <LoaderCircle className="animate-spin" /> : <CloudDownload />}
                {selectedCount > 0
                  ? t("overview.fetchSelected", { count: fetchable })
                  : t("overview.fetchAll")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null || pullable === 0}
                title={t("overview.pullAllTip")}
                onClick={() => void run("pull")}
              >
                {busy === "pull" ? <LoaderCircle className="animate-spin" /> : <ArrowDownToLine />}
                {selectedCount > 0
                  ? t("overview.pullSelected", { count: pullable })
                  : t("overview.pullAll", { count: pullable })}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null || pushable.length === 0}
                title={t("overview.pushTip")}
                onClick={() => openAction("push")}
              >
                <ArrowUpFromLine />
                {t("overview.push", { count: pushable.length })}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null || targets.length === 0}
                title={t("overview.commitTip")}
                onClick={() => openAction("commit")}
              >
                <GitCommitHorizontal />
                {t("overview.commit")}
              </Button>
              <div className="flex-1" />
              <Button
                size="sm"
                variant="ghost"
                disabled={overview.isFetching}
                onClick={() => void overview.refetch()}
              >
                <RefreshCw className={cn(overview.isFetching && "animate-spin")} />
                {t("overview.refresh")}
              </Button>
            </div>
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs">
              <ToggleGroup
                type="single"
                size="sm"
                variant="outline"
                value={filter}
                aria-label={t("overview.filter.label")}
                onValueChange={(v) => v && setFilter(v as OverviewFilter)}
              >
                {FILTERS.map((f) => (
                  <ToggleGroupItem key={f} value={f} className="h-6 px-2 text-[11px]">
                    {t(`overview.filter.${f}`, { count: counts[f] })}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
              <ToggleGroup
                type="single"
                size="sm"
                variant="outline"
                value={groupMode}
                aria-label={t("overview.group.label")}
                onValueChange={(v) => v && changeGroup(v as GroupMode)}
              >
                {GROUP_MODES.map((m) => (
                  <ToggleGroupItem key={m} value={m} className="h-6 px-2 text-[11px]">
                    {t(`overview.group.${m}`)}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
              <div className="flex-1" />
              {selectedCount > 0 && (
                <span className="flex items-center gap-2 text-muted-foreground">
                  {t("overview.selected", { count: selectedCount })}
                  <Button size="xs" variant="ghost" onClick={() => setSelected(new Set())}>
                    {t("overview.clearSelection")}
                  </Button>
                </span>
              )}
            </div>
            {overview.isError && (
              <p className="text-xs text-danger">{errorMessage(overview.error)}</p>
            )}
            <div className="max-h-[50vh] overflow-y-auto rounded-md border">
              <table className="w-full text-xs">
                <thead className="sticky top-0 z-10 bg-surface text-left text-[11px] text-muted-foreground uppercase">
                  <tr>
                    <th className="w-7 px-2 py-1.5">
                      <TriCheckbox
                        state={checkState(shownIds, selected)}
                        label={t("overview.selectAll")}
                        onChange={(on) => select(shownIds, on)}
                      />
                    </th>
                    <th className="px-2 py-1.5 font-semibold">{t("overview.col.repo")}</th>
                    <th className="px-2 py-1.5 font-semibold">{t("overview.col.branch")}</th>
                    <th className="px-2 py-1.5 font-semibold">{t("overview.col.sync")}</th>
                    <th className="px-2 py-1.5 font-semibold">{t("overview.col.state")}</th>
                  </tr>
                </thead>
                {groups.map((group) => {
                  const ids = group.rows.map((r) => r.repoId);
                  const isCollapsed = collapsed.has(group.key);
                  return (
                    <tbody key={group.key}>
                      {groupMode !== "none" && (
                        <tr className="border-t bg-muted/40">
                          <td className="px-2 py-1">
                            <TriCheckbox
                              state={checkState(ids, selected)}
                              label={t("overview.group.select", { name: group.label })}
                              onChange={(on) => select(ids, on)}
                            />
                          </td>
                          <td colSpan={4} className="px-1 py-1">
                            <button
                              type="button"
                              className="flex max-w-full items-center gap-1.5 rounded px-1 font-semibold hover:bg-accent"
                              aria-expanded={!isCollapsed}
                              title={group.title}
                              onClick={() => toggleGroup(group.key)}
                            >
                              {isCollapsed ? (
                                <ChevronRight className="size-3.5 shrink-0" />
                              ) : (
                                <ChevronDown className="size-3.5 shrink-0" />
                              )}
                              {group.color && (
                                <span
                                  className="size-2 shrink-0 rounded-full"
                                  style={{ backgroundColor: group.color }}
                                />
                              )}
                              <span className="truncate">{group.label}</span>
                              <span className="shrink-0 font-normal text-muted-foreground">
                                {t("overview.group.count", { count: group.rows.length })}
                              </span>
                            </button>
                          </td>
                        </tr>
                      )}
                      {!isCollapsed &&
                        group.rows.map((row) => (
                          <tr
                            key={row.repoId}
                            tabIndex={0}
                            className={cn(
                              "cursor-pointer border-t outline-none hover:bg-accent focus-visible:bg-accent",
                              row.repoId === activeId && "bg-accent/50",
                            )}
                            title={t("overview.openTip", { path: row.path })}
                            onClick={() => go(row.repoId)}
                            onKeyDown={(e) => e.key === "Enter" && go(row.repoId)}
                          >
                            <td className="px-2 py-1.5">
                              <TriCheckbox
                                state={selected.has(row.repoId) ? "all" : "none"}
                                label={t("overview.selectRepo", { name: row.name })}
                                onChange={(on) => select([row.repoId], on)}
                              />
                            </td>
                            <td className="px-2 py-1.5">
                              <span className="flex items-center gap-1.5 font-medium">
                                <FolderGit2 className="size-3.5 shrink-0 text-muted-foreground" />
                                <span className="truncate">{row.name}</span>
                              </span>
                            </td>
                            <td className="max-w-48 truncate px-2 py-1.5">
                              {row.branch ?? (
                                <span className="text-muted-foreground">
                                  {row.missing || row.error ? "" : t("overview.detached")}
                                </span>
                              )}
                            </td>
                            <td className="px-2 py-1.5">
                              <SyncCell row={row} />
                            </td>
                            <td className="px-2 py-1.5">
                              <StateCell row={row} />
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  );
                })}
                {groups.length === 0 && !overview.isPending && (
                  <tbody>
                    <tr>
                      <td colSpan={5} className="px-2 py-6 text-center text-muted-foreground">
                        {rows.length === 0 ? t("overview.empty") : t("overview.filter.empty")}
                      </td>
                    </tr>
                  </tbody>
                )}
              </table>
            </div>
            <p className="text-[11px] text-muted-foreground">{t("overview.note")}</p>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
