import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { TriangleAlert } from "lucide-react";
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
import { commands, errorMessage, type RebasePreview } from "@/ipc";
import { runRebase } from "./actions";
import { canStartRebase } from "./model";
import type { RebaseDialog as Request } from "./store";

const LISTED = 8;

export function Warning({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex gap-2 rounded-md border border-warning/40 bg-warning/10 p-2 text-xs">
      <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" />
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

/** Body of the plain rebase confirmation, given a preview (tested on its own). */
export function RebaseSummary({
  preview,
  ontoLabel,
  allowPushed,
  autostash,
  onAllowPushed,
  onAutostash,
}: {
  preview: RebasePreview;
  ontoLabel: string;
  allowPushed: boolean;
  autostash: boolean;
  onAllowPushed: (v: boolean) => void;
  onAutostash: (v: boolean) => void;
}) {
  const { t } = useTranslation();
  const branch = preview.branch ?? "HEAD";
  if (preview.upToDate) {
    return <p className="text-xs">{t("rebase.dialog.upToDate", { branch, onto: ontoLabel })}</p>;
  }
  return (
    <div className="space-y-2 text-xs">
      {!preview.isCurrent && <p>{t("rebase.dialog.checkoutFirst", { branch })}</p>}
      {preview.fastForward ? (
        <p>{t("rebase.dialog.fastForward", { branch, onto: ontoLabel })}</p>
      ) : (
        <>
          <p className="font-semibold">{t("rebase.dialog.commits", { count: preview.total })}</p>
          <ul className="max-h-40 space-y-0.5 overflow-y-auto rounded-md border p-1.5">
            {preview.commits.slice(-LISTED).map((c) => (
              <li key={c.oid} className="flex items-center gap-2 truncate">
                <span className="font-mono text-muted-foreground">{c.shortOid}</span>
                <span className="min-w-0 flex-1 truncate">{c.subject}</span>
                {c.pushed && (
                  <span className="shrink-0 rounded bg-warning/15 px-1 text-[10px] text-warning">
                    {t("rebase.dialog.pushed")}
                  </span>
                )}
              </li>
            ))}
            {preview.total > LISTED && (
              <li className="text-muted-foreground italic">
                {t("rebase.dialog.more", { count: preview.total - LISTED })}
              </li>
            )}
          </ul>
        </>
      )}
      {preview.merges > 0 && <p>{t("rebase.dialog.merges", { count: preview.merges })}</p>}
      {preview.pushed > 0 && (
        <Warning>
          <p>{t("rebase.dialog.pushedWarn", { count: preview.pushed })}</p>
          <label className="mt-1.5 flex items-center gap-1.5 font-medium">
            <input
              type="checkbox"
              checked={allowPushed}
              onChange={(e) => onAllowPushed(e.target.checked)}
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
              onChange={(e) => onAutostash(e.target.checked)}
              className="accent-primary"
            />
            {t("rebase.dialog.autostash")}
          </label>
        </Warning>
      )}
    </div>
  );
}

export function RebaseDialog({
  dialog,
  onClose,
}: {
  dialog: Extract<Request, { kind: "rebase" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { repoId, onto, ontoLabel, branch } = dialog;
  const [allowPushed, setAllowPushed] = useState(false);
  const [autostash, setAutostash] = useState(false);
  const [busy, setBusy] = useState(false);
  const preview = useQuery({
    queryKey: ["rebase-preview", repoId, onto, branch],
    queryFn: () => commands.rebasePreview(repoId, onto, branch),
    enabled: isTauri(),
    staleTime: 0,
    gcTime: 0,
  });
  const out = preview.data;
  const p = out?.status === "done" ? out.value : null;
  const label = branch ?? p?.branch ?? "HEAD";

  const start = async () => {
    if (!p) return;
    setBusy(true);
    const done = await runRebase(
      repoId,
      () =>
        commands.rebaseStart(repoId, {
          onto: p.onto,
          branch: branch ?? null,
          autostash,
          allowPushed,
        }),
      p.branch,
    );
    setBusy(false);
    if (done) onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("rebase.dialog.title", { branch: label, onto: ontoLabel })}</DialogTitle>
          <DialogDescription>
            {t("rebase.dialog.hint", { branch: label, onto: ontoLabel })}
          </DialogDescription>
        </DialogHeader>
        {preview.isPending && isTauri() && (
          <p className="text-xs text-muted-foreground">{t("rebase.dialog.loading")}</p>
        )}
        {preview.isError && <p className="text-xs text-danger">{errorMessage(preview.error)}</p>}
        {out?.status === "rejected" && (
          <p className="text-xs text-danger">{failureText(out.reason, out.message)}</p>
        )}
        {p && (
          <RebaseSummary
            preview={p}
            ontoLabel={ontoLabel}
            allowPushed={allowPushed}
            autostash={autostash}
            onAllowPushed={setAllowPushed}
            onAutostash={setAutostash}
          />
        )}
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {p?.upToDate ? t("rebase.dialog.close") : t("common.cancel")}
          </Button>
          {!p?.upToDate && (
            <Button
              size="sm"
              disabled={!p || busy || !canStartRebase(p, allowPushed, autostash)}
              onClick={() => void start()}
            >
              {t("rebase.dialog.start")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
