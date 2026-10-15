import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import {
  FolderOpen,
  FolderSymlink,
  Loader2,
  RefreshCw,
  RotateCcw,
  Trash2,
  TriangleAlert,
  Users,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import {
  commands,
  errorMessage,
  events,
  type CleanReport,
  type StorageKind,
  type StorageLocation,
  type StorageTargetCheck,
} from "@/ipc";
import { formatBytes } from "@/lib/format";
import { problemText, progressPercent } from "./model";
import { storageKeys, useStorageOverview, useStorageStatus } from "./queries";
import { useStorageUi } from "./store";
import { UserDataSection } from "./UserDataSection";

interface PendingChange {
  kind: StorageKind;
  /** `null` = back to the default location. */
  path: string | null;
  check: StorageTargetCheck;
}

interface Message {
  tone: "error" | "info";
  text: string;
  detail?: string;
}

/** Settings → "Lưu trữ": log / cache / app-data folders (PLAN 3B.3). */
export function StorageDialog() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const open = useStorageUi((s) => s.open);
  const setOpen = useStorageUi((s) => s.setOpen);
  const progress = useStorageUi((s) => s.progress);
  const setProgress = useStorageUi((s) => s.setProgress);
  const overview = useStorageOverview(open);
  const status = useStorageStatus().data;
  const [checking, setChecking] = useState<StorageKind | null>(null);
  const [change, setChange] = useState<PendingChange | null>(null);
  const [moving, setMoving] = useState(false);
  const [clean, setClean] = useState<"cache" | "logs" | null>(null);
  const [cleaning, setCleaning] = useState(false);
  const [restartAsk, setRestartAsk] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);

  useEffect(() => {
    if (!open) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    void events.storageMoveProgress
      .listen((e) => setProgress(e.payload))
      .then((un) => {
        if (cancelled) un();
        else stop = un;
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [open, setProgress]);

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: storageKeys.overview });
    void qc.invalidateQueries({ queryKey: storageKeys.status });
  };

  const check = async (kind: StorageKind, path: string | null) => {
    setChecking(kind);
    setMessage(null);
    try {
      const result = await commands.storageCheckTarget(kind, path);
      setChange({ kind, path, check: result });
    } catch (e) {
      setMessage({ tone: "error", text: t("storage.checkFailed"), detail: errorMessage(e) });
    } finally {
      setChecking(null);
    }
  };

  const pick = async (kind: StorageKind) => {
    const dir = await openFolderDialog({
      directory: true,
      multiple: false,
      title: t("storage.pickTitle", { kind: t(`storage.kind.${kind}`) }),
    });
    if (typeof dir === "string") await check(kind, dir);
  };

  const applyChange = async () => {
    if (!change) return;
    const { kind, path } = change;
    setMoving(true);
    setProgress(null);
    try {
      const result = await commands.storageChange(kind, path);
      setChange(null);
      if (result.problem) {
        setMessage({ tone: "error", text: problemText(t, result.problem) });
      } else if (result.restartRequired) {
        setMessage({ tone: "info", text: t("storage.dataScheduled", { path: result.target }) });
      } else {
        const left = result.oldFilesLeft
          ? " " + t("storage.oldFilesLeft", { count: result.oldFilesLeft })
          : "";
        setMessage({
          tone: "info",
          text: t("storage.moved", { kind: t(`storage.kind.${kind}`), path: result.target }) + left,
        });
      }
    } catch (e) {
      setChange(null);
      setMessage({ tone: "error", text: t("storage.moveFailed"), detail: errorMessage(e) });
    } finally {
      setMoving(false);
      setProgress(null);
      refresh();
    }
  };

  const runClean = async () => {
    if (!clean) return;
    setCleaning(true);
    try {
      const report: CleanReport =
        clean === "cache" ? await commands.storageClearCache() : await commands.storagePruneLogs();
      setMessage({
        tone: report.failed ? "error" : "info",
        text:
          t(clean === "cache" ? "storage.cacheCleared" : "storage.logsPruned", {
            count: report.removedFiles,
            size: formatBytes(report.removedBytes),
          }) + (report.failed ? " " + t("storage.cleanFailed", { count: report.failed }) : ""),
      });
    } catch (e) {
      setMessage({ tone: "error", text: t("storage.cleanError"), detail: errorMessage(e) });
    } finally {
      setCleaning(false);
      setClean(null);
      refresh();
    }
  };

  const openFolder = async (kind: StorageKind) => {
    try {
      await commands.storageOpen(kind);
    } catch (e) {
      setMessage({ tone: "error", text: t("storage.openFailed"), detail: errorMessage(e) });
    }
  };

  const cancelPending = async () => {
    try {
      await commands.storageCancelPending();
      setMessage({ tone: "info", text: t("storage.pendingCancelled") });
    } catch (e) {
      setMessage({ tone: "error", text: t("storage.moveFailed"), detail: errorMessage(e) });
    } finally {
      refresh();
    }
  };

  const busy = moving || checking !== null || cleaning || Boolean(overview.data?.busy);
  const pendingData = status?.pendingData ?? null;

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => !moving && setOpen(o)}>
        <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("storage.title")}</DialogTitle>
            <DialogDescription>{t("storage.description")}</DialogDescription>
          </DialogHeader>

          {pendingData && (
            <div
              className="space-y-2 rounded-md border border-primary/40 bg-primary/5 p-3 text-xs"
              role="status"
            >
              <div className="font-medium">{t("storage.restartNeeded")}</div>
              <div className="break-all text-muted-foreground">
                {t("storage.restartNeededHint", { path: pendingData })}
              </div>
              <div className="flex flex-wrap gap-2">
                <Button size="sm" className="h-7 text-xs" onClick={() => setRestartAsk(true)}>
                  <RefreshCw />
                  {t("storage.restartNow")}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 text-xs"
                  onClick={() => void cancelPending()}
                >
                  {t("storage.cancelPending")}
                </Button>
              </div>
            </div>
          )}

          {message && (
            <div
              role={message.tone === "error" ? "alert" : "status"}
              className={
                message.tone === "error"
                  ? "rounded-md border border-destructive/40 bg-destructive/5 p-2.5 text-xs text-destructive"
                  : "rounded-md border bg-muted/40 p-2.5 text-xs"
              }
            >
              <div className="break-words">{message.text}</div>
              {message.detail && (
                <details className="mt-1 text-muted-foreground">
                  <summary className="cursor-pointer select-none">{t("notice.technical")}</summary>
                  <pre className="mt-1 font-mono text-[11px] break-all whitespace-pre-wrap">
                    {message.detail}
                  </pre>
                </details>
              )}
            </div>
          )}

          {progress && moving && (
            <div
              className="space-y-1 text-xs"
              role="progressbar"
              aria-valuenow={progressPercent(progress)}
            >
              <div className="flex justify-between text-muted-foreground">
                <span>
                  {t(`storage.phase.${progress.phase}`)} · {t(`storage.kind.${progress.kind}`)}
                </span>
                <span>
                  {formatBytes(progress.doneBytes)} / {formatBytes(progress.totalBytes)}
                </span>
              </div>
              <div className="h-1.5 overflow-hidden rounded bg-muted">
                <div
                  className="h-full bg-primary transition-all"
                  style={{ width: `${progressPercent(progress)}%` }}
                />
              </div>
            </div>
          )}

          <div className="space-y-3">
            {overview.isPending && overview.fetchStatus !== "idle" ? (
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Loader2 className="size-3.5 animate-spin" />
                {t("storage.measuring")}
              </div>
            ) : overview.isError ? (
              <div className="text-xs text-destructive" role="alert">
                {t("storage.loadFailed")}: {errorMessage(overview.error)}
              </div>
            ) : (
              overview.data?.locations.map((loc) => (
                <LocationRow
                  key={`loc-${loc.kind}`}
                  loc={loc}
                  busy={busy}
                  checking={checking === loc.kind}
                  onOpen={() => void openFolder(loc.kind)}
                  onChange={() => void pick(loc.kind)}
                  onReset={() => void check(loc.kind, null)}
                  onClean={
                    loc.kind === "cache"
                      ? () => setClean("cache")
                      : loc.kind === "log"
                        ? () => setClean("logs")
                        : undefined
                  }
                />
              ))
            )}

            <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border p-3 text-xs">
              <div className="min-w-0 space-y-0.5">
                <div className="font-medium">{t("storage.cloneTitle")}</div>
                <div className="text-muted-foreground">{t("storage.cloneHint")}</div>
              </div>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={() => {
                  setOpen(false);
                  useAccountDialogs.getState().openManager();
                }}
              >
                <Users />
                {t("storage.openAccounts")}
              </Button>
            </div>

            <UserDataSection onLeave={() => setOpen(false)} />
          </div>
        </DialogContent>
      </Dialog>

      {change && (
        <ConfirmDialog
          open
          title={t(change.path === null ? "storage.resetTitle" : "storage.changeTitle", {
            kind: t(`storage.kind.${change.kind}`),
          })}
          description={
            change.check.problem
              ? t("storage.cannotUse")
              : t(change.kind === "data" ? "storage.changeDataBody" : "storage.changeNowBody")
          }
          confirmLabel={t(change.kind === "data" ? "storage.schedule" : "storage.moveNow")}
          busy={moving || change.check.problem !== null}
          onConfirm={() => void applyChange()}
          onCancel={() => !moving && setChange(null)}
        >
          <div className="space-y-1.5 text-xs">
            <div>
              <span className="text-muted-foreground">{t("storage.target")}: </span>
              <span className="font-mono break-all">{change.check.target}</span>
            </div>
            {change.check.createsSubfolder && (
              <div className="text-muted-foreground">{t("storage.subfolderNote")}</div>
            )}
            {!change.check.problem && (
              <div className="text-muted-foreground">
                {t("storage.sizes", {
                  needed: formatBytes(change.check.neededBytes),
                  free: change.check.freeBytes !== null ? formatBytes(change.check.freeBytes) : "–",
                })}
              </div>
            )}
            {change.check.problem && (
              <div className="flex items-start gap-1.5 text-destructive" role="alert">
                <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
                <span>{problemText(t, change.check.problem)}</span>
              </div>
            )}
            {moving && progress && (
              <div className="text-muted-foreground">
                {t(`storage.phase.${progress.phase}`)} — {progressPercent(progress)}%
              </div>
            )}
          </div>
        </ConfirmDialog>
      )}

      <ConfirmDialog
        open={clean !== null}
        title={t(clean === "cache" ? "storage.clearCacheTitle" : "storage.pruneLogsTitle")}
        description={t(clean === "cache" ? "storage.clearCacheBody" : "storage.pruneLogsBody")}
        confirmLabel={t(clean === "cache" ? "storage.clearCache" : "storage.pruneLogs")}
        destructive
        busy={cleaning}
        onConfirm={() => void runClean()}
        onCancel={() => !cleaning && setClean(null)}
      />

      <ConfirmDialog
        open={restartAsk}
        title={t("storage.restartTitle")}
        description={t("storage.restartBody")}
        confirmLabel={t("storage.restartNow")}
        onConfirm={() => {
          setRestartAsk(false);
          void commands.storageRestart();
        }}
        onCancel={() => setRestartAsk(false)}
      />
    </>
  );
}

interface RowProps {
  loc: StorageLocation;
  busy: boolean;
  checking: boolean;
  onOpen: () => void;
  onChange: () => void;
  onReset: () => void;
  onClean?: () => void;
}

function LocationRow({ loc, busy, checking, onOpen, onChange, onReset, onClean }: RowProps) {
  const { t } = useTranslation();
  const used = `${loc.usageComplete ? "" : "≥ "}${formatBytes(loc.usedBytes)}`;
  return (
    <section
      className="space-y-1.5 rounded-md border p-3 text-xs"
      aria-label={t(`storage.kind.${loc.kind}`)}
      data-testid={`storage-${loc.kind}`}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <div className="flex items-baseline gap-2">
          <span className="text-sm font-medium">{t(`storage.kind.${loc.kind}`)}</span>
          <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {t(loc.isDefault ? "storage.default" : "storage.custom")}
          </span>
        </div>
        <span className="font-medium tabular-nums">
          {t("storage.used", { size: used, count: loc.fileCount })}
        </span>
      </div>
      <div className="text-muted-foreground">{t(`storage.about.${loc.kind}`)}</div>
      <div className="font-mono text-[11px] break-all">{loc.path}</div>
      {loc.freeBytes !== null && (
        <div className="text-muted-foreground">
          {t("storage.free", { size: formatBytes(loc.freeBytes) })}
        </div>
      )}
      {loc.pendingPath && (
        <div className="break-all text-primary">
          {t("storage.pendingRow", { path: loc.pendingPath })}
        </div>
      )}
      <div className="flex flex-wrap gap-1.5 pt-1">
        <Button variant="outline" size="sm" className="h-7 text-xs" onClick={onOpen}>
          <FolderOpen />
          {t("storage.open")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={busy}
          onClick={onChange}
        >
          {checking ? <Loader2 className="animate-spin" /> : <FolderSymlink />}
          {t("storage.change")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={busy || loc.isDefault}
          onClick={onReset}
        >
          <RotateCcw />
          {t("storage.reset")}
        </Button>
        {onClean && (
          <Button
            variant="outline"
            size="sm"
            className="h-7 text-xs text-destructive"
            disabled={busy}
            onClick={onClean}
          >
            <Trash2 />
            {t(loc.kind === "cache" ? "storage.clearCache" : "storage.pruneLogs")}
          </Button>
        )}
      </div>
    </section>
  );
}
