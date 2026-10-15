import { Download, ExternalLink, Loader2, TriangleAlert } from "lucide-react";
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
import { openExternal } from "@/features/accounts/manager/openExternal";
import { formatBytes } from "@/lib/format";
import { installUpdate, postponeUpdate, skipUpdate } from "./actions";
import { useStartupCheck, useUpdateProgressEvents } from "./hooks";
import { cleanNotes, versionLabel } from "./notes";
import { useUpdater } from "./store";
// Registers the "Kiểm tra cập nhật" palette command.
import "./palette";

/** Mounted once in `App.tsx`: startup check, progress events and the update dialog. */
export function UpdaterHost() {
  useStartupCheck();
  useUpdateProgressEvents();
  const open = useUpdater((s) => s.update !== null);
  const installing = useUpdater((s) => s.installing);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && !installing && postponeUpdate()}>
      {open && <UpdateDialog />}
    </Dialog>
  );
}

export function UpdateDialog() {
  const { t, i18n } = useTranslation();
  const update = useUpdater((s) => s.update);
  const currentVersion = useUpdater((s) => s.currentVersion);
  const installing = useUpdater((s) => s.installing);
  const progress = useUpdater((s) => s.progress);
  const error = useUpdater((s) => s.error);
  if (!update) return null;

  const notes = cleanNotes(update.notes);
  const date =
    update.date !== null ? new Date(update.date * 1000).toLocaleDateString(i18n.language) : null;
  const percent = progress?.percent ?? 0;
  const phase = progress?.phase ?? "downloading";

  return (
    <DialogContent
      className="sm:max-w-lg"
      showCloseButton={!installing}
      onEscapeKeyDown={(e) => installing && e.preventDefault()}
      onInteractOutside={(e) => installing && e.preventDefault()}
    >
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <Download className="size-4 text-primary" />
          {t("updater.dialog.title", { version: versionLabel(update.version) })}
        </DialogTitle>
        <DialogDescription>
          {t("updater.dialog.current", { version: versionLabel(currentVersion) })}
          {date && <> · {t("updater.dialog.date", { date })}</>}
        </DialogDescription>
      </DialogHeader>

      <div className="space-y-3">
        <div className="text-xs font-semibold">{t("updater.dialog.notes")}</div>
        <div
          className="max-h-64 overflow-y-auto rounded-md border bg-muted/40 p-3 text-xs break-words whitespace-pre-wrap"
          data-testid="update-notes"
        >
          {notes || <span className="text-muted-foreground">{t("updater.dialog.noNotes")}</span>}
        </div>
        {update.releaseUrl && (
          <Button
            variant="link"
            size="sm"
            className="h-auto px-0 text-xs"
            onClick={() => void openExternal(update.releaseUrl ?? "")}
          >
            <ExternalLink className="size-3.5" />
            {t("updater.dialog.viewOnGithub")}
          </Button>
        )}
        {update.skipped && !installing && (
          <p className="text-xs text-muted-foreground">{t("updater.dialog.skippedNote")}</p>
        )}

        {installing ? (
          <div className="space-y-1.5" role="status" aria-live="polite">
            <div className="flex items-center gap-2 text-xs">
              <Loader2 className="size-3.5 animate-spin" />
              {t(`updater.phase.${phase}`)}
              {phase === "downloading" && progress && progress.total !== null && (
                <span className="text-muted-foreground">
                  {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
                </span>
              )}
            </div>
            <div
              className="h-1.5 overflow-hidden rounded-full bg-muted"
              role="progressbar"
              aria-label={t("updater.phase.downloading")}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percent}
            >
              <div
                className="h-full bg-primary transition-[width]"
                style={{ width: `${percent}%` }}
              />
            </div>
            <p className="text-[11px] text-muted-foreground">{t("updater.dialog.restartHint")}</p>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">{t("updater.dialog.saveHint")}</p>
        )}

        {error && (
          <div
            className="space-y-1 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs"
            role="alert"
          >
            <div className="flex items-center gap-1.5 font-medium">
              <TriangleAlert className="size-3.5 text-destructive" />
              {t(`updater.error.${error.kind}`)}
            </div>
            <div className="whitespace-pre-line">{t(`updater.errorHelp.${error.kind}`)}</div>
            {error.message.trim() && (
              <details className="text-muted-foreground">
                <summary className="cursor-pointer select-none">{t("notice.technical")}</summary>
                <div className="mt-1 font-mono break-all">{error.message}</div>
              </details>
            )}
          </div>
        )}
      </div>

      <DialogFooter className="gap-2 sm:justify-between">
        <Button variant="ghost" size="sm" disabled={installing} onClick={() => void skipUpdate()}>
          {t("updater.dialog.skip")}
        </Button>
        <div className="flex flex-col-reverse gap-2 sm:flex-row">
          <Button variant="outline" size="sm" disabled={installing} onClick={postponeUpdate}>
            {t("updater.dialog.later")}
          </Button>
          <Button size="sm" disabled={installing} onClick={() => void installUpdate()}>
            {installing ? <Loader2 className="animate-spin" /> : <Download />}
            {error ? t("updater.dialog.retry") : t("updater.dialog.install")}
          </Button>
        </div>
      </DialogFooter>
    </DialogContent>
  );
}
