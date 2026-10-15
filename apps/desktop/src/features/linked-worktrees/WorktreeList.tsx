import { useState } from "react";
import {
  Copy,
  Eraser,
  FolderOpen,
  FolderTree,
  GitBranch,
  LoaderCircle,
  Lock,
  LockOpen,
  Plus,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { errorMessage, type Worktree } from "@/ipc";
import { openRepository } from "@/features/repos/actions";
import { cn } from "@/lib/utils";
import {
  notifyNothingToPrune,
  previewPrune,
  pruneWorktrees,
  removeWorktree,
  setWorktreeLocked,
} from "./actions";
import { AddWorktreeDialog } from "./AddWorktreeDialog";
import { folderName, worktreeLabel } from "./model";
import { useLinkedWorktreeEvents, useLinkedWorktrees } from "./queries";

/** Rows of the sidebar "Worktrees" section (linked worktrees, `git worktree`). */
export function WorktreeList({ repoId, filter = "" }: { repoId: number; filter?: string }) {
  const { t } = useTranslation();
  const list = useLinkedWorktrees(repoId);
  useLinkedWorktreeEvents(repoId);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<{ w: Worktree; force: boolean } | null>(null);
  const [pruning, setPruning] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);

  if (list.isPending) {
    return (
      <li className="flex items-center gap-1.5 py-1 pr-2 pl-8 text-[11px] text-muted-foreground italic">
        <LoaderCircle className="size-3 animate-spin" />
        {t("worktrees.loading")}
      </li>
    );
  }
  if (list.isError) {
    return (
      <li className="py-1 pr-2 pl-8 text-[11px] text-danger">
        {t("worktrees.error", { message: errorMessage(list.error) })}
      </li>
    );
  }
  const all = list.data ?? [];
  const q = filter.trim().toLowerCase();
  const shown = all.filter(
    (w) => !q || w.path.toLowerCase().includes(q) || (w.branch ?? "").toLowerCase().includes(q),
  );

  const confirmRemove = async () => {
    const r = removing;
    if (!r) return;
    setRemoving(null);
    const result = await removeWorktree(repoId, r.w.path, r.force);
    if (result === "dirty") setRemoving({ w: r.w, force: true });
  };

  const startPrune = async () => {
    setBusy(true);
    const lines = await previewPrune(repoId);
    setBusy(false);
    if (!lines) return;
    if (lines.length === 0) notifyNothingToPrune();
    else setPruning(lines);
  };

  return (
    <>
      {shown.map((w) => (
        <ContextMenu key={w.path}>
          <ContextMenuTrigger asChild>
            <li
              title={[
                w.path,
                w.isMain ? t("worktrees.main") : "",
                w.locked
                  ? t("worktrees.lockedTitle", { reason: w.lockReason ?? t("worktrees.noReason") })
                  : "",
                w.prunable ? t("worktrees.prunableTitle") : "",
              ]
                .filter(Boolean)
                .join("\n")}
              onDoubleClick={() => !w.isCurrent && !w.prunable && void openRepository(w.path)}
              className={cn(
                "flex h-6 cursor-default items-center gap-2 pr-2 pl-8 text-xs hover:bg-surface-2 data-[state=open]:bg-surface-2",
                w.isCurrent && "font-semibold",
              )}
            >
              <FolderTree
                className={cn(
                  "size-3.5 shrink-0",
                  w.prunable ? "text-warning" : "text-muted-foreground",
                )}
              />
              <span className={cn("min-w-0 flex-1 truncate", w.prunable && "line-through")}>
                {folderName(w.path)}
              </span>
              <span className="flex max-w-[45%] shrink-0 items-center gap-0.5 truncate text-[10px] text-muted-foreground">
                <GitBranch className="size-3 shrink-0" />
                <span className="truncate">
                  {w.detached
                    ? t("worktrees.detached", { oid: worktreeLabel(w) })
                    : worktreeLabel(w)}
                </span>
              </span>
              {w.locked && <Lock className="size-3 shrink-0 text-muted-foreground" />}
              {w.prunable && <TriangleAlert className="size-3 shrink-0 text-warning" />}
            </li>
          </ContextMenuTrigger>
          <ContextMenuContent className="w-64">
            <ContextMenuLabel className="truncate">{w.path}</ContextMenuLabel>
            <ContextMenuItem
              disabled={w.isCurrent || w.prunable}
              onSelect={() => void openRepository(w.path)}
            >
              <FolderOpen />
              {t("worktrees.open")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(w.path)}>
              <Copy />
              {t("worktrees.copyPath")}
            </ContextMenuItem>
            {!w.isMain && (
              <>
                <ContextMenuSeparator />
                {w.locked ? (
                  <ContextMenuItem
                    onSelect={() => void setWorktreeLocked(repoId, w.path, false, null)}
                  >
                    <LockOpen />
                    {t("worktrees.unlock")}
                  </ContextMenuItem>
                ) : (
                  <ContextMenuItem
                    onSelect={() => void setWorktreeLocked(repoId, w.path, true, null)}
                  >
                    <Lock />
                    {t("worktrees.lock")}
                  </ContextMenuItem>
                )}
                <ContextMenuItem
                  variant="destructive"
                  disabled={w.isCurrent || w.locked}
                  onSelect={() => setRemoving({ w, force: false })}
                >
                  <Trash2 />
                  {t("worktrees.remove")}
                </ContextMenuItem>
                {w.locked && <ContextMenuLabel>{t("worktrees.unlockFirst")}</ContextMenuLabel>}
                {w.isCurrent && <ContextMenuLabel>{t("worktrees.openOther")}</ContextMenuLabel>}
              </>
            )}
          </ContextMenuContent>
        </ContextMenu>
      ))}
      <li className="flex gap-1 px-2 pt-1 pl-7">
        <Button
          size="xs"
          variant="ghost"
          className="text-muted-foreground"
          onClick={() => setAdding(true)}
        >
          <Plus />
          {t("worktrees.add")}
        </Button>
        {all.length > 1 && (
          <Button
            size="xs"
            variant="ghost"
            className="text-muted-foreground"
            disabled={busy}
            title={t("worktrees.pruneHint")}
            onClick={() => void startPrune()}
          >
            {busy ? <LoaderCircle className="animate-spin" /> : <Eraser />}
            {t("worktrees.prune")}
          </Button>
        )}
      </li>
      <AddWorktreeDialog
        key={adding ? "open" : "closed"}
        repoId={repoId}
        worktrees={all}
        open={adding}
        onClose={() => setAdding(false)}
      />
      <ConfirmDialog
        open={removing !== null}
        title={t(removing?.force ? "worktrees.forceTitle" : "worktrees.removeTitle", {
          folder: removing ? folderName(removing.w.path) : "",
        })}
        description={t(removing?.force ? "worktrees.forceBody" : "worktrees.removeBody", {
          path: removing?.w.path ?? "",
        })}
        confirmLabel={t(removing?.force ? "worktrees.forceConfirm" : "worktrees.remove")}
        destructive
        onConfirm={() => void confirmRemove()}
        onCancel={() => setRemoving(null)}
      />
      <ConfirmDialog
        open={pruning !== null}
        title={t("worktrees.pruneTitle", { count: pruning?.length ?? 0 })}
        description={t("worktrees.pruneBody")}
        confirmLabel={t("worktrees.prune")}
        onConfirm={() => {
          setPruning(null);
          void pruneWorktrees(repoId);
        }}
        onCancel={() => setPruning(null)}
      >
        <ul className="max-h-40 overflow-auto rounded border bg-surface-2/40 p-2 font-mono text-[11px]">
          {(pruning ?? []).map((l) => (
            <li key={l} className="truncate" title={l}>
              {l}
            </li>
          ))}
        </ul>
      </ConfirmDialog>
    </>
  );
}
