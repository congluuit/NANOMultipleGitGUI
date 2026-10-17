import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArchiveRestore,
  FileText,
  LoaderCircle,
  PackageOpen,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useRepoRefs } from "@/features/repos/queries";
import { useWorktreeStatus } from "@/features/worktree/queries";
import type { Stash } from "@/ipc";
import { cn } from "@/lib/utils";
import { applyStash, dropStash, stashName } from "./actions";
import { StashFilesDialog } from "./StashFilesDialog";
import { useStashUi } from "./store";
import { defaultStash, hasLocalChanges, stashMeta, stashTitle } from "./useStashToolbar";

function BranchChip({ stash, current }: { stash: Stash; current: string | null }) {
  const { t } = useTranslation();
  if (!stash.branch) return null;
  const here = stash.branch === current;
  return (
    <span
      className={cn(
        "shrink-0 rounded px-1 text-[10px] leading-4 font-medium",
        here ? "bg-success/15 text-success" : "bg-warning/15 text-warning",
      )}
      title={here ? t("stash.picker.thisBranch") : t("stash.picker.otherBranch")}
    >
      {here ? t("stash.picker.thisBranchShort", { branch: stash.branch }) : stash.branch}
    </span>
  );
}

/** The picker wired to `useStashUi` (mounted once, in the toolbar). */
export function StashPickerDialogHost() {
  const repoId = useStashUi((s) => s.pickerFor);
  // A fresh selection every time the dialog opens.
  return <StashPickerDialog key={repoId ?? "closed"} repoId={repoId} />;
}

/**
 * "Choose a stash" dialog of the toolbar Pop button (several stashes, or one made on another
 * branch): newest first, the entry's branch and age, Pop / Apply / files / drop.
 */
function StashPickerDialog({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const close = useStashUi((s) => s.closePicker);
  const open = repoId !== null;
  const refs = useRepoRefs(repoId, open);
  const status = useWorktreeStatus(repoId ?? -1, open);
  const stashes = useMemo(
    () => [...(refs.data?.stashes ?? [])].sort((a, b) => a.index - b.index),
    [refs.data?.stashes],
  );
  const branch = refs.data?.head.branch ?? null;
  const [picked, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [showing, setShowing] = useState<number | null>(null);
  const [dropping, setDropping] = useState<Stash | null>(null);
  const list = useRef<HTMLUListElement>(null);

  // Until the user picks one (or when the picked one is gone): the default entry.
  const selected = stashes.some((s) => s.index === picked) ? picked : defaultStash(stashes, branch);
  // Nothing left to choose (last one popped / dropped elsewhere): close.
  useEffect(() => {
    if (open && refs.data && stashes.length === 0) close();
  }, [open, refs.data, stashes.length, close]);

  const current = stashes.find((s) => s.index === selected) ?? null;
  const dirty = hasLocalChanges(status.data);

  const run = async (pop: boolean) => {
    if (repoId === null || !current) return;
    setBusy(true);
    try {
      await applyStash(repoId, current.index, pop);
    } finally {
      setBusy(false);
    }
    close();
  };

  const move = (delta: number) => {
    if (!stashes.length) return;
    const at = stashes.findIndex((s) => s.index === selected);
    const next = stashes[Math.min(stashes.length - 1, Math.max(0, at + delta))];
    if (!next) return;
    setSelected(next.index);
    list.current
      ?.querySelector<HTMLElement>(`[data-stash-index="${next.index}"]`)
      ?.scrollIntoView({ block: "nearest" });
  };

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => !o && close()}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("stash.picker.title", { count: stashes.length })}</DialogTitle>
            <DialogDescription>{t("stash.picker.desc")}</DialogDescription>
          </DialogHeader>
          {dirty && (
            <p className="flex items-start gap-2 rounded-md bg-warning/10 px-2 py-1.5 text-xs text-warning">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
              {t("stash.picker.dirty")}
            </p>
          )}
          {current?.branch && branch && current.branch !== branch && (
            <p className="flex items-start gap-2 rounded-md bg-warning/10 px-2 py-1.5 text-xs text-warning">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
              {t("stash.picker.otherBranchWarn", { branch: current.branch, current: branch })}
            </p>
          )}
          <ul
            ref={list}
            role="listbox"
            aria-label={t("stash.picker.title", { count: stashes.length })}
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                move(1);
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                move(-1);
              } else if (e.key === "Enter" && !busy) {
                e.preventDefault();
                void run(true);
              }
            }}
            className="max-h-80 overflow-auto rounded-md border focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
          >
            {stashes.map((s) => {
              const active = s.index === selected;
              return (
                <li
                  key={s.index}
                  role="option"
                  aria-selected={active}
                  data-stash-index={s.index}
                  onClick={() => setSelected(s.index)}
                  onDoubleClick={() => {
                    setSelected(s.index);
                    if (!busy && repoId !== null) {
                      setBusy(true);
                      void applyStash(repoId, s.index, true).finally(() => {
                        setBusy(false);
                        close();
                      });
                    }
                  }}
                  className={cn(
                    "grid cursor-default gap-0.5 border-b px-3 py-2 text-xs last:border-b-0",
                    active
                      ? "bg-primary/10 ring-1 ring-primary/50 ring-inset"
                      : "hover:bg-surface-2",
                  )}
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                      {stashName(s.index)}
                    </span>
                    <span className="min-w-0 flex-1 truncate font-medium" title={s.message}>
                      {stashTitle(s)}
                    </span>
                    <BranchChip stash={s} current={branch} />
                  </span>
                  {s.time > 0 && (
                    <span className="text-[11px] text-muted-foreground">
                      {stashMeta({ ...s, branch: null })}
                      {s.index === 0 && ` · ${t("stash.picker.newest")}`}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
          <DialogFooter className="flex-wrap items-center gap-y-2 sm:justify-between">
            <span className="flex flex-wrap gap-1.5">
              <Button
                size="sm"
                variant="ghost"
                disabled={!current}
                onClick={() => current && setShowing(current.index)}
              >
                <FileText />
                {t("stash.showFiles")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="text-danger hover:text-danger"
                disabled={!current || busy}
                onClick={() => current && setDropping(current)}
              >
                <Trash2 />
                {t("stash.drop")}
              </Button>
            </span>
            <span className="flex flex-wrap gap-1.5">
              <Button
                size="sm"
                variant="outline"
                disabled={!current || busy}
                onClick={() => void run(false)}
                title={t("stash.picker.applyHint")}
              >
                <PackageOpen />
                {t("stash.apply")}
              </Button>
              <Button
                size="sm"
                disabled={!current || busy}
                onClick={() => void run(true)}
                title={t("stash.picker.popHint")}
              >
                {busy ? <LoaderCircle className="animate-spin" /> : <ArchiveRestore />}
                {t("stash.picker.pop", { name: current ? stashName(current.index) : "" })}
              </Button>
            </span>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {repoId !== null && (
        <StashFilesDialog repoId={repoId} index={showing} onClose={() => setShowing(null)} />
      )}
      <ConfirmDialog
        open={dropping !== null}
        title={t("stash.dropTitle", { name: stashName(dropping?.index ?? 0) })}
        description={t("stash.dropBody", { message: dropping?.message ?? "" })}
        confirmLabel={t("stash.dropConfirm")}
        destructive
        onConfirm={() => {
          const target = dropping;
          setDropping(null);
          if (target && repoId !== null) void dropStash(repoId, target.index);
        }}
        onCancel={() => setDropping(null)}
      />
    </>
  );
}
