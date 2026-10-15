import { useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { Eye, GitCommitHorizontal, History, LoaderCircle, UserRound, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { fileStatusLetter, splitPath } from "@/features/commit/format";
import { DiffView } from "@/features/diff/DiffView";
import { formatDate } from "@/features/graph/draw";
import { commands, errorMessage, type FileHistoryEntry } from "@/ipc";
import { cn } from "@/lib/utils";
import { closeFileView, jumpToCommit, openFileView } from "@/stores/fileView";

const PAGE_SIZE = 100;

const historyKeys = {
  history: (repoId: number, path: string) => ["fileHistory", repoId, path] as const,
};

const STATUS_CLASS: Record<string, string> = {
  A: "text-success",
  M: "text-warning",
  D: "text-danger",
  R: "text-info",
  C: "text-info",
  T: "text-warning",
};

/** Commits that changed a file (following renames) with the file's diff in each. */
export function FileHistoryView({ repoId, path }: { repoId: number; path: string }) {
  const { t, i18n } = useTranslation();
  const [selected, setSelected] = useState<FileHistoryEntry | null>(null);
  const history = useInfiniteQuery({
    queryKey: historyKeys.history(repoId, path),
    queryFn: ({ pageParam }) => commands.fileHistory(repoId, path, null, pageParam, PAGE_SIZE),
    initialPageParam: 0,
    getNextPageParam: (last, pages) =>
      last.hasMore ? pages.reduce((n, p) => n + p.entries.length, 0) : undefined,
    enabled: isTauri(),
  });
  const entries = history.data?.pages.flatMap((p) => p.entries) ?? [];
  const current = selected ?? entries[0] ?? null;
  const { dir, name } = splitPath(path);

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background" data-testid="file-history">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <History className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-24 flex-1 truncate font-medium" title={path}>
          <span className="mr-1 text-muted-foreground">{t("fileHistory.label")}</span>
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </span>
        {entries.length > 0 && (
          <span className="shrink-0 text-[11px] text-muted-foreground">
            {t("fileHistory.commits", { count: entries.length })}
            {history.hasNextPage ? "+" : ""}
          </span>
        )}
        <Button
          variant="outline"
          size="xs"
          onClick={() => openFileView(repoId, { kind: "blame", path, rev: null })}
        >
          <UserRound />
          {t("fileHistory.blame")}
        </Button>
        <button
          type="button"
          onClick={() => closeFileView(repoId)}
          title={t("fileHistory.close")}
          aria-label={t("fileHistory.close")}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-4" />
        </button>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="flex w-80 shrink-0 flex-col border-r">
          {history.isError ? (
            <p className="p-4 text-xs text-danger">
              {t("fileHistory.error", { message: errorMessage(history.error) })}
            </p>
          ) : history.isPending ? (
            <p className="flex items-center gap-2 p-4 text-xs text-muted-foreground">
              <LoaderCircle className="size-4 animate-spin" />
              {t("fileHistory.loading")}
            </p>
          ) : entries.length === 0 ? (
            <p className="p-4 text-xs text-muted-foreground">{t("fileHistory.empty")}</p>
          ) : (
            <ScrollArea className="min-h-0 flex-1">
              <ul role="listbox" aria-label={t("fileHistory.label")}>
                {entries.map((e) => {
                  const letter = fileStatusLetter(e.status);
                  const active = current?.oid === e.oid;
                  return (
                    <li
                      key={e.oid}
                      role="option"
                      aria-selected={active}
                      tabIndex={0}
                      onClick={() => setSelected(e)}
                      onKeyDown={(ev) => {
                        if (ev.key === "Enter" || ev.key === " ") {
                          ev.preventDefault();
                          setSelected(e);
                        }
                      }}
                      className={cn(
                        "group cursor-default border-b px-3 py-1.5 text-xs hover:bg-surface-2",
                        active && "bg-primary/10 shadow-[inset_2px_0_0_var(--primary)]",
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <span
                          className={cn("w-3 shrink-0 font-mono font-bold", STATUS_CLASS[letter])}
                        >
                          {letter}
                        </span>
                        <span className="min-w-0 flex-1 truncate font-medium" title={e.summary}>
                          {e.summary}
                        </span>
                      </div>
                      <div className="flex items-center gap-2 pl-5 text-[11px] text-muted-foreground">
                        <span className="font-mono">{e.oid.slice(0, 7)}</span>
                        <span className="min-w-0 flex-1 truncate">{e.author.name}</span>
                        <span className="shrink-0">{formatDate(e.author.time, i18n.language)}</span>
                      </div>
                      {e.oldPath && (
                        <div className="truncate pl-5 text-[11px] text-info">
                          {t("fileHistory.renamedFrom", { path: e.oldPath })}
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
              {history.hasNextPage && (
                <div className="p-2">
                  <Button
                    variant="outline"
                    size="xs"
                    className="w-full"
                    disabled={history.isFetchingNextPage}
                    onClick={() => void history.fetchNextPage()}
                  >
                    {history.isFetchingNextPage && <LoaderCircle className="animate-spin" />}
                    {t("fileHistory.loadMore")}
                  </Button>
                </div>
              )}
            </ScrollArea>
          )}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          {current ? (
            <>
              <div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b bg-surface px-3 py-1.5 text-xs">
                <span className="min-w-0 flex-1 truncate">
                  <span className="mr-1 font-mono text-muted-foreground">
                    {current.oid.slice(0, 7)}
                  </span>
                  {current.summary}
                </span>
                <Button
                  variant="outline"
                  size="xs"
                  onClick={() => jumpToCommit(repoId, current.oid)}
                >
                  <GitCommitHorizontal />
                  {t("fileHistory.showInGraph")}
                </Button>
                <Button
                  variant="outline"
                  size="xs"
                  disabled={current.status === "deleted"}
                  onClick={() =>
                    openFileView(repoId, { kind: "blame", path: current.path, rev: current.oid })
                  }
                >
                  <UserRound />
                  {t("fileHistory.blameHere")}
                </Button>
                <Button
                  variant="outline"
                  size="xs"
                  disabled={current.status === "deleted"}
                  onClick={() =>
                    openFileView(repoId, { kind: "file", path: current.path, commit: current.oid })
                  }
                >
                  <Eye />
                  {t("fileHistory.viewFile")}
                </Button>
              </div>
              <DiffView
                key={current.oid}
                repoId={repoId}
                request={{
                  side: "commit",
                  path: current.path,
                  oldPath: current.oldPath ?? undefined,
                  commit: current.oid,
                }}
                onClose={() => closeFileView(repoId)}
              />
            </>
          ) : (
            <p className="flex flex-1 items-center justify-center p-6 text-xs text-muted-foreground">
              {t("fileHistory.pick")}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
