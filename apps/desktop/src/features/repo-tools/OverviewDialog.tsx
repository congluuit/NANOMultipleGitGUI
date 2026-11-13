import { useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  CloudDownload,
  ArrowDownToLine,
  FolderGit2,
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
import { errorMessage, type RepoOverview } from "@/ipc";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";
import { canFetch, fetchAllRepos, overviewTotals, pullAllRepos } from "./overview";
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

/** Every open repository at a glance, with Fetch all / Pull all. */
export function OverviewDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const overview = useWorkspaceOverview(true);
  const rows = overview.data ?? [];
  const activeId = useWorkspace((s) => s.activeId);
  const [busy, setBusy] = useState<"fetch" | "pull" | null>(null);
  const totals = overviewTotals(rows);
  const fetchable = rows.filter(canFetch).length;

  const run = async (kind: "fetch" | "pull") => {
    setBusy(kind);
    try {
      await (kind === "fetch" ? fetchAllRepos(rows) : pullAllRepos(rows));
    } finally {
      setBusy(null);
    }
  };
  const go = (row: RepoOverview) => {
    useWorkspace.getState().setActive(row.repoId);
    onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("overview.title")}</DialogTitle>
          <DialogDescription>
            {rows.length > 0
              ? t("overview.summary", {
                  count: rows.length,
                  behind: totals.behind,
                  ahead: totals.ahead,
                  dirty: totals.dirty,
                })
              : t("overview.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            disabled={busy !== null || fetchable === 0}
            onClick={() => void run("fetch")}
          >
            {busy === "fetch" ? <LoaderCircle className="animate-spin" /> : <CloudDownload />}
            {t("overview.fetchAll")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy !== null || totals.pullable === 0}
            title={t("overview.pullAllTip")}
            onClick={() => void run("pull")}
          >
            {busy === "pull" ? <LoaderCircle className="animate-spin" /> : <ArrowDownToLine />}
            {t("overview.pullAll", { count: totals.pullable })}
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
        {overview.isError && <p className="text-xs text-danger">{errorMessage(overview.error)}</p>}
        <div className="max-h-[55vh] overflow-y-auto rounded-md border">
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-surface text-left text-[11px] text-muted-foreground uppercase">
              <tr>
                <th className="px-2 py-1.5 font-semibold">{t("overview.col.repo")}</th>
                <th className="px-2 py-1.5 font-semibold">{t("overview.col.branch")}</th>
                <th className="px-2 py-1.5 font-semibold">{t("overview.col.sync")}</th>
                <th className="px-2 py-1.5 font-semibold">{t("overview.col.state")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.repoId}
                  tabIndex={0}
                  className={cn(
                    "cursor-pointer border-t outline-none hover:bg-accent focus-visible:bg-accent",
                    row.repoId === activeId && "bg-accent/50",
                  )}
                  title={t("overview.openTip", { path: row.path })}
                  onClick={() => go(row)}
                  onKeyDown={(e) => e.key === "Enter" && go(row)}
                >
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
              {rows.length === 0 && !overview.isPending && (
                <tr>
                  <td colSpan={4} className="px-2 py-6 text-center text-muted-foreground">
                    {t("overview.empty")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <p className="text-[11px] text-muted-foreground">{t("overview.note")}</p>
      </DialogContent>
    </Dialog>
  );
}
