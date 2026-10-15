import { useState } from "react";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { FileViewMenuItems } from "@/features/file-history/FileMenuItems";
import { commands, errorMessage, type StatusEntry } from "@/ipc";
import { openFileView, useFileView } from "@/stores/fileView";
import { useOpenDiff, useWorktreeStore } from "@/stores/worktree";
import { CommitBox } from "./CommitBox";
import { FileSection } from "./FileSection";
import {
  discardPaths,
  invalidateWorktree,
  requestFor,
  runOp,
  unstagePaths,
  useWorktreeStatus,
} from "./queries";

export function WorkingTreePanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const status = useWorktreeStatus(repoId);
  const openDiff = useWorktreeStore((s) => s.openDiff);
  const diff = useOpenDiff(repoId);
  const fileView = useFileView(repoId);
  const [discarding, setDiscarding] = useState<StatusEntry[] | null>(null);
  const st = status.data;

  const activeIn = (side: "staged" | "unstaged") =>
    diff &&
    (side === "staged"
      ? diff.side === "staged"
      : diff.side === "unstaged" || diff.side === "untracked")
      ? diff.path
      : null;

  const stage = (entries: StatusEntry[]) =>
    void runOp(repoId, () =>
      commands.stageFiles(
        repoId,
        entries.map((e) => e.path),
      ),
    );
  const unstage = (entries: StatusEntry[]) =>
    void runOp(repoId, () => commands.unstageFiles(repoId, unstagePaths(entries)));

  const confirmDiscard = async () => {
    const entries = discarding ?? [];
    setDiscarding(null);
    const { tracked, untracked } = discardPaths(entries);
    await runOp(repoId, () => commands.discardFiles(repoId, tracked, untracked));
    if (diff && entries.some((e) => e.path === diff.path) && diff.side !== "staged") {
      openDiff(repoId, null);
    }
  };

  const total = st ? st.staged.length + st.unstaged.length + st.conflicted.length : 0;

  return (
    // On short windows the file list keeps a usable height and the whole panel scrolls instead.
    <aside
      className="flex min-w-0 flex-1 flex-col overflow-y-auto bg-surface"
      aria-label={t("wip.title")}
    >
      <div className="flex h-8 shrink-0 items-center justify-between border-b px-3 text-xs font-semibold">
        <span>
          {t("wip.title")}
          {st && total > 0 && (
            <span className="ml-1.5 font-normal text-muted-foreground">
              {t("wip.count", { count: total })}
            </span>
          )}
        </span>
        <button
          type="button"
          title={t("wip.refresh")}
          aria-label={t("wip.refresh")}
          onClick={() => invalidateWorktree(repoId)}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <RefreshCw className={status.isFetching ? "size-3.5 animate-spin" : "size-3.5"} />
        </button>
      </div>
      {status.isPending && (
        <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          {t("wip.loading")}
        </div>
      )}
      {status.isError && (
        <div className="flex-1 p-4 text-xs text-danger">
          {t("wip.error", { message: errorMessage(status.error) })}
        </div>
      )}
      {st && (
        <ScrollArea className="min-h-28 flex-1">
          {st.conflicted.length > 0 && (
            <FileSection
              kind="conflicted"
              entries={st.conflicted}
              activePath={fileView?.kind === "conflict" ? fileView.path : null}
              onOpen={(e) => openFileView(repoId, { kind: "conflict", path: e.path })}
              onPrimary={stage}
              onPrimaryAll={() => stage(st.conflicted)}
              menuExtra={(target) => (
                <FileViewMenuItems
                  repoId={repoId}
                  path={target.length === 1 ? target[0]!.path : null}
                  conflicted
                />
              )}
            />
          )}
          <FileSection
            kind="unstaged"
            entries={st.unstaged}
            activePath={activeIn("unstaged")}
            onOpen={(e) => openDiff(repoId, requestFor(e, false))}
            onPrimary={stage}
            onPrimaryAll={() => void runOp(repoId, () => commands.stageAll(repoId))}
            onDiscard={setDiscarding}
            menuExtra={(target) => (
              <FileViewMenuItems
                repoId={repoId}
                path={
                  target.length === 1 && target[0]!.kind !== "untracked" ? target[0]!.path : null
                }
                deleted={target[0]?.kind === "deleted"}
              />
            )}
          />
          <FileSection
            kind="staged"
            entries={st.staged}
            activePath={activeIn("staged")}
            onOpen={(e) => openDiff(repoId, requestFor(e, true))}
            onPrimary={unstage}
            onPrimaryAll={() => void runOp(repoId, () => commands.unstageAll(repoId))}
            menuExtra={(target) => (
              <FileViewMenuItems
                repoId={repoId}
                path={target.length === 1 && target[0]!.kind !== "added" ? target[0]!.path : null}
                deleted={target[0]?.kind === "deleted"}
              />
            )}
          />
          {total === 0 && (
            <p className="px-3 py-4 text-center text-xs text-muted-foreground">{t("wip.clean")}</p>
          )}
        </ScrollArea>
      )}
      <CommitBox
        repoId={repoId}
        stagedCount={st?.staged.length ?? 0}
        conflictCount={st?.conflicted.length ?? 0}
        hasHead={!!st && !st.unborn}
        stagedPaths={st?.staged.map((e) => e.path)}
      />
      <ConfirmDialog
        open={discarding !== null}
        title={t("wip.discardTitle")}
        description={t("wip.discardBody", { count: discarding?.length ?? 0 })}
        confirmLabel={t("wip.discardConfirm")}
        destructive
        onConfirm={() => void confirmDiscard()}
        onCancel={() => setDiscarding(null)}
      >
        {discarding && (
          <ul className="max-h-40 overflow-y-auto rounded-md border bg-background p-2 font-mono text-[11px]">
            {discarding.slice(0, 50).map((e) => (
              <li key={e.path} className="truncate">
                {e.path}
              </li>
            ))}
            {discarding.length > 50 && <li>…</li>}
          </ul>
        )}
      </ConfirmDialog>
    </aside>
  );
}
