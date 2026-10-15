import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { failureText } from "@/features/commits/outcome";
import { commands, errorMessage, type RebasePreview, type TodoStep } from "@/ipc";
import { runRebase } from "./actions";
import {
  initialRows,
  isUnchanged,
  previewResult,
  toSteps,
  validate,
  type TodoError,
  type TodoRow,
} from "./model";
import { Warning } from "./RebaseDialog";
import type { RebaseDialog } from "./store";
import { TodoEditor } from "./TodoEditor";

/** Commits the backend lists in detail (see `rebase::MAX_LISTED`). */
const MAX_ROWS = 2000;

export interface InteractiveSubmit {
  steps: TodoStep[];
  autostash: boolean;
  allowPushed: boolean;
}

/** The dialog for a loaded todo list (tested on its own). */
export function InteractiveRebaseView({
  preview,
  baseLabel,
  busy,
  preset,
  onSubmit,
  onCancel,
}: {
  preview: RebasePreview;
  baseLabel: string;
  busy?: boolean;
  preset?: { oid: string; action: "drop" | "reword" };
  onSubmit: (s: InteractiveSubmit) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<TodoRow[]>(() =>
    initialRows(preview.commits).map((r) =>
      preset && r.oid.startsWith(preset.oid) ? { ...r, action: preset.action } : r,
    ),
  );
  const [allowPushed, setAllowPushed] = useState(false);
  const [autostash, setAutostash] = useState(false);
  const errors = useMemo(() => validate(rows), [rows]);
  const result = useMemo(() => previewResult(rows), [rows]);
  const unchanged = isUnchanged(rows, preview.commits);
  const pushedTouched = rows.filter((r, i) => r.pushed && changedAt(rows, preview, i)).length;
  const tooMany = preview.total > Math.min(preview.commits.length, MAX_ROWS);
  const blocked =
    errors.length > 0 ||
    unchanged ||
    tooMany ||
    (pushedTouched > 0 && !allowPushed) ||
    (preview.dirty && !autostash);
  const branch = preview.branch ?? "HEAD";

  const errorText = (e: TodoError) =>
    e.kind === "merges"
      ? t("rebase.interactive.mergesBlocked")
      : e.kind === "squashFirst"
        ? t("rebase.interactive.errorSquashFirst")
        : t("rebase.interactive.errorEmptyMessage", { oid: e.oid });

  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("rebase.interactive.title", { branch })}</DialogTitle>
          <DialogDescription>
            {t("rebase.interactive.base", { base: baseLabel })} {t("rebase.interactive.help")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid min-h-0 flex-1 gap-3 overflow-hidden sm:grid-cols-[1fr_15rem]">
          <div className="min-h-0 overflow-y-auto pr-1">
            {tooMany ? (
              <Warning>
                {t("rebase.interactive.tooMany", { count: preview.total, max: MAX_ROWS })}
              </Warning>
            ) : (
              <TodoEditor rows={rows} onChange={setRows} />
            )}
          </div>
          <aside className="min-h-0 space-y-2 overflow-y-auto text-xs" aria-live="polite">
            <p className="font-semibold">
              {t("rebase.interactive.preview", { count: result.commits.length })}
            </p>
            <ol
              className="space-y-0.5 rounded-md border p-1.5"
              aria-label={t("rebase.interactive.preview", { count: result.commits.length })}
            >
              {result.commits.map((c) => (
                <li key={c.oid} className="truncate">
                  <span className="mr-1 font-mono text-muted-foreground">{c.shortOid}</span>
                  {c.subject}
                  {(c.folded > 0 || c.reworded || c.stops) && (
                    <span className="block pl-2 text-[10px] text-muted-foreground">
                      {[
                        c.folded > 0 && t("rebase.interactive.previewFolded", { count: c.folded }),
                        c.reworded && t("rebase.interactive.previewReworded"),
                        c.stops && t("rebase.interactive.previewStops"),
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                    </span>
                  )}
                </li>
              ))}
            </ol>
            {result.dropped > 0 && (
              <p className="text-muted-foreground">
                {t("rebase.interactive.previewDropped", { count: result.dropped })}
              </p>
            )}
            {unchanged && (
              <p className="text-muted-foreground">{t("rebase.interactive.unchanged")}</p>
            )}
          </aside>
        </div>
        <div className="space-y-2">
          {errors.map((e) => (
            <p key={JSON.stringify(e)} role="alert" className="text-xs text-danger">
              {errorText(e)}
            </p>
          ))}
          {pushedTouched > 0 && (
            <Warning>
              <p>{t("rebase.dialog.pushedWarn", { count: pushedTouched })}</p>
              <label className="mt-1.5 flex items-center gap-1.5 font-medium">
                <input
                  type="checkbox"
                  checked={allowPushed}
                  onChange={(e) => setAllowPushed(e.target.checked)}
                  className="accent-primary"
                />
                {t("rebase.dialog.pushedConfirm")}
              </label>
            </Warning>
          )}
          {preview.dirty && (
            <Warning>
              <p>{t("rebase.dialog.dirtyWarn")}</p>
              <label className="mt-1.5 flex items-center gap-1.5 font-medium">
                <input
                  type="checkbox"
                  checked={autostash}
                  onChange={(e) => setAutostash(e.target.checked)}
                  className="accent-primary"
                />
                {t("rebase.dialog.autostash")}
              </label>
            </Warning>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button
            size="sm"
            disabled={blocked || busy}
            onClick={() =>
              onSubmit({ steps: toSteps(rows), autostash, allowPushed: pushedTouched > 0 })
            }
          >
            {t("rebase.interactive.start")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Whether row `i` gets rewritten: git replays every commit from the first changed position on
 * (a commit keeps its id only while everything before it is untouched).
 */
function changedAt(rows: TodoRow[], preview: RebasePreview, i: number): boolean {
  const first = rows.findIndex((r, k) => r.action !== "pick" || r.oid !== preview.commits[k]?.oid);
  return first !== -1 && i >= first;
}

export function InteractiveRebaseDialog({
  dialog,
  onClose,
}: {
  dialog: Extract<RebaseDialog, { kind: "interactive" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { repoId, base, baseLabel, preset } = dialog;
  const [busy, setBusy] = useState(false);
  const todo = useQuery({
    queryKey: ["rebase-todo", repoId, base],
    queryFn: () => commands.rebaseTodo(repoId, base),
    enabled: isTauri(),
    staleTime: 0,
    gcTime: 0,
  });
  const out = todo.data;

  if (out?.status === "done") {
    const p = out.value;
    const label = p.onto.startsWith(base) ? baseLabel : `${p.onto.slice(0, 7)}`;
    return (
      <InteractiveRebaseView
        preview={p}
        baseLabel={label}
        busy={busy}
        preset={preset}
        onCancel={onClose}
        onSubmit={async ({ steps, autostash, allowPushed }) => {
          setBusy(true);
          const done = await runRebase(
            repoId,
            () =>
              commands.rebaseInteractive(repoId, { base: p.onto, steps, autostash, allowPushed }),
            p.branch,
          );
          setBusy(false);
          if (done) onClose();
        }}
      />
    );
  }

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("rebase.interactive.title", { branch: "HEAD" })}</DialogTitle>
          <DialogDescription>{t("rebase.interactive.base", { base: baseLabel })}</DialogDescription>
        </DialogHeader>
        {todo.isPending && (
          <p className="text-xs text-muted-foreground">{t("rebase.dialog.loading")}</p>
        )}
        {todo.isError && <p className="text-xs text-danger">{errorMessage(todo.error)}</p>}
        {out?.status === "rejected" && (
          <p className="text-xs text-danger">{failureText(out.reason, out.message)}</p>
        )}
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("rebase.dialog.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
