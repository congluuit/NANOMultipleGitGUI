import { useMemo, useState, type ReactNode } from "react";
import {
  Download,
  FileType,
  HardDrive,
  LoaderCircle,
  Lock,
  LockOpen,
  Plus,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Unlink,
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  commands,
  errorMessage,
  type FsckReport,
  type LfsFile,
  type LfsLock,
  type LfsPattern,
  type PruneReport,
} from "@/ipc";
import { useGuardDialogs } from "@/features/guards/store";
import { pushNeedsReview } from "@/features/guards/model";
import { useRepoRefs } from "@/features/repos/queries";
import { runLfsFetchAll } from "@/features/transfers/actions";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import { trackPattern, untrackPattern } from "./actions";
import {
  downloadable,
  fileState,
  fileTotals,
  filterFiles,
  groupProblems,
  locksByPath,
} from "./lfsModel";
import { downloadFiles, lockFile, setLockable, unlockFile } from "./lockActions";
import { useLfsManager, type LfsTab } from "./managerStore";
import { canUntrack } from "./patterns";
import {
  invalidateLfs,
  lfsKeys,
  useLfsFiles,
  useLfsLocks,
  useLfsStatus,
  useLfsStorage,
} from "./queries";
import { TrackPatternDialog } from "./TrackPatternDialog";
import { queryClient } from "@/lib/queryClient";

const TABS: LfsTab[] = ["files", "locks", "patterns", "maintenance"];

/** Mount once: the LFS manager (patterns / files / locks / maintenance) of `useLfsManager`. */
export function LfsManagerHost() {
  const repoId = useLfsManager((s) => s.repoId);
  const tab = useLfsManager((s) => s.tab);
  const setTab = useLfsManager((s) => s.setTab);
  const close = useLfsManager((s) => s.close);
  const { t } = useTranslation();
  return (
    <Dialog open={repoId !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent className="flex h-[80vh] flex-col gap-3 sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>{t("lfs.managerTitle")}</DialogTitle>
          <DialogDescription>{t("lfs.managerDesc")}</DialogDescription>
        </DialogHeader>
        <div role="tablist" className="flex gap-1 border-b">
          {TABS.map((k) => (
            <button
              key={k}
              role="tab"
              type="button"
              aria-selected={tab === k}
              onClick={() => setTab(k)}
              className={cn(
                "-mb-px border-b-2 px-3 py-1.5 text-xs",
                tab === k
                  ? "border-primary font-semibold text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )}
            >
              {t(`lfs.tab.${k}`)}
            </button>
          ))}
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {repoId !== null && tab === "files" && <FilesTab repoId={repoId} />}
          {repoId !== null && tab === "locks" && <LocksTab repoId={repoId} />}
          {repoId !== null && tab === "patterns" && <PatternsTab repoId={repoId} />}
          {repoId !== null && tab === "maintenance" && <MaintenanceTab repoId={repoId} />}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Note({
  tone = "muted",
  children,
}: {
  tone?: "muted" | "danger" | "ok";
  children: ReactNode;
}) {
  return (
    <p
      className={cn(
        "rounded-md border px-2 py-1.5 text-xs",
        tone === "danger" && "border-danger/40 bg-danger/10 text-danger",
        tone === "ok" && "border-success/40 bg-success/10 text-success",
        tone === "muted" && "text-muted-foreground",
      )}
    >
      {children}
    </p>
  );
}

function LockChip({ lock }: { lock: LfsLock }) {
  const { t } = useTranslation();
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-0.5 rounded px-1 text-[10px] font-semibold",
        lock.ours ? "bg-info/15 text-info" : "bg-warning/15 text-warning",
      )}
      title={t(lock.ours ? "lfsLocks.byYou" : "lfsLocks.byOther", { owner: lock.owner ?? "?" })}
    >
      <Lock className="size-2.5" />
      {lock.ours ? t("lfsLocks.you") : (lock.owner ?? "?")}
    </span>
  );
}

// ----- files ---------------------------------------------------------------------------------

function FilesTab({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const files = useLfsFiles(repoId);
  const lockQuery = useLfsLocks(repoId);
  const locks = useMemo(
    () => locksByPath(lockQuery.data?.status === "done" ? lockQuery.data.value : null),
    [lockQuery.data],
  );
  const [text, setText] = useState("");
  const [pointersOnly, setPointersOnly] = useState(false);
  const [lockedOnly, setLockedOnly] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [noSpace, setNoSpace] = useState<{ paths: string[]; needed: number; free: number } | null>(
    null,
  );
  const [forceUnlock, setForceUnlock] = useState<LfsLock | null>(null);

  if (files.isPending) return <Note>{t("lfsFiles.loading")}</Note>;
  if (files.isError)
    return <Note tone="danger">{t("lfsFiles.error", { message: errorMessage(files.error) })}</Note>;
  const all = files.data ?? [];
  const shown = filterFiles(all, locks, { text, pointersOnly, lockedOnly });
  const totals = fileTotals(all);
  const toDownload = downloadable(all, selected);

  const download = async (paths: string[]) => {
    const r = await downloadFiles(repoId, paths);
    if (r !== "started") setNoSpace({ paths, ...r.noSpace });
    else setSelected(new Set());
  };

  const toggle = (path: string) =>
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="text-muted-foreground">
          {t("lfsFiles.summary", {
            count: totals.count,
            size: formatBytes(totals.bytes),
            pointers: totals.pointers,
          })}
        </span>
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("lfsFiles.filter")}
          className="h-7 min-w-40 flex-1 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
        />
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            className="accent-primary"
            checked={pointersOnly}
            onChange={(e) => setPointersOnly(e.target.checked)}
          />
          {t("lfsFiles.pointersOnly")}
        </label>
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            className="accent-primary"
            checked={lockedOnly}
            onChange={(e) => setLockedOnly(e.target.checked)}
          />
          {t("lfsFiles.lockedOnly")}
        </label>
        <Button
          size="xs"
          variant="outline"
          disabled={toDownload.length === 0}
          onClick={() => void download(toDownload)}
        >
          <Download />
          {t("lfsFiles.downloadSelected", { count: toDownload.length })}
        </Button>
        <Button
          size="xs"
          variant="ghost"
          onClick={() => {
            invalidateLfs(repoId);
            void queryClient.invalidateQueries({ queryKey: lfsKeys.locks(repoId) });
          }}
          title={t("lfsFiles.refresh")}
        >
          <RefreshCw />
        </Button>
      </div>
      {lockQuery.data?.status === "failed" && (
        <Note>{t(`lfsLocks.failure.${lockQuery.data.failure.kind}`, { path: "" })}</Note>
      )}
      {all.length === 0 && <Note>{t("lfsFiles.none")}</Note>}
      <ul className="divide-y rounded-md border">
        {shown.map((f) => (
          <FileRow
            key={f.path}
            file={f}
            lock={locks.get(f.path)}
            selected={selected.has(f.path)}
            onToggle={() => toggle(f.path)}
            onDownload={() => void download([f.path])}
            onLock={() => void lockFile(repoId, f.path)}
            onUnlock={(lock) =>
              lock.ours
                ? void unlockFile(repoId, { path: f.path, id: lock.id || null }, false)
                : setForceUnlock(lock)
            }
          />
        ))}
      </ul>
      <ConfirmDialog
        open={noSpace !== null}
        title={t("lfsFiles.noSpaceTitle")}
        description={t("lfsFiles.noSpaceBody", {
          needed: formatBytes(noSpace?.needed ?? 0),
          free: formatBytes(noSpace?.free ?? 0),
        })}
        confirmLabel={t("lfsFiles.downloadAnyway")}
        onConfirm={() => {
          const n = noSpace;
          setNoSpace(null);
          if (n) void downloadFiles(repoId, n.paths, true);
        }}
        onCancel={() => setNoSpace(null)}
      />
      <ForceUnlockDialog
        lock={forceUnlock}
        onConfirm={(lock) => {
          setForceUnlock(null);
          void unlockFile(repoId, { path: lock.path, id: lock.id || null }, true);
        }}
        onCancel={() => setForceUnlock(null)}
      />
    </div>
  );
}

function FileRow({
  file,
  lock,
  selected,
  onToggle,
  onDownload,
  onLock,
  onUnlock,
}: {
  file: LfsFile;
  lock: LfsLock | undefined;
  selected: boolean;
  onToggle: () => void;
  onDownload: () => void;
  onLock: () => void;
  onUnlock: (lock: LfsLock) => void;
}) {
  const { t } = useTranslation();
  const state = fileState(file);
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <li className="flex h-7 items-center gap-2 px-2 text-xs hover:bg-surface-2 data-[state=open]:bg-surface-2">
          <input
            type="checkbox"
            className="accent-primary"
            checked={selected}
            onChange={onToggle}
            aria-label={file.path}
          />
          <FileType className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate font-mono" title={file.path}>
            {file.path}
          </span>
          {file.lockable && !lock && (
            <span className="shrink-0 text-[10px] text-muted-foreground">{t("lfs.lockable")}</span>
          )}
          {lock && <LockChip lock={lock} />}
          <span
            className={cn(
              "shrink-0 rounded px-1 text-[10px]",
              state === "downloaded" ? "text-success" : "bg-warning/15 text-warning",
            )}
            title={t(`lfsFiles.stateHint.${state}`)}
          >
            {t(`lfsFiles.state.${state}`)}
          </span>
          <span className="w-16 shrink-0 text-right text-muted-foreground tabular-nums">
            {formatBytes(file.size)}
          </span>
          {state !== "downloaded" ? (
            <Button size="xs" variant="ghost" onClick={onDownload} title={t("lfsFiles.download")}>
              <Download />
            </Button>
          ) : (
            <span className="w-7 shrink-0" />
          )}
        </li>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-60">
        <ContextMenuLabel className="truncate font-mono">{file.path}</ContextMenuLabel>
        <ContextMenuItem disabled={state === "downloaded"} onSelect={onDownload}>
          <Download />
          {t("lfsFiles.download")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        {lock ? (
          <ContextMenuItem
            variant={lock.ours ? "default" : "destructive"}
            onSelect={() => onUnlock(lock)}
          >
            <LockOpen />
            {t(lock.ours ? "lfsLocks.unlock" : "lfsLocks.forceUnlock")}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={onLock}>
            <Lock />
            {t("lfsLocks.lock")}
          </ContextMenuItem>
        )}
        <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(file.path)}>
          <FileType />
          {t("lfsFiles.copyPath")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

function ForceUnlockDialog({
  lock,
  onConfirm,
  onCancel,
}: {
  lock: LfsLock | null;
  onConfirm: (lock: LfsLock) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  return (
    <ConfirmDialog
      open={lock !== null}
      title={t("lfsLocks.forceTitle", { path: lock?.path ?? "" })}
      description={t("lfsLocks.forceBody", { owner: lock?.owner ?? "?" })}
      confirmLabel={t("lfsLocks.forceUnlock")}
      destructive
      onConfirm={() => lock && onConfirm(lock)}
      onCancel={onCancel}
    />
  );
}

// ----- locks ---------------------------------------------------------------------------------

function LocksTab({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const q = useLfsLocks(repoId);
  const [lockPath, setLockPath] = useState("");
  const [forceUnlock, setForceUnlock] = useState<LfsLock | null>(null);

  const refresh = () => void queryClient.invalidateQueries({ queryKey: lfsKeys.locks(repoId) });

  return (
    <div className="grid gap-2">
      <div className="flex items-center gap-2 text-xs">
        <input
          value={lockPath}
          onChange={(e) => setLockPath(e.target.value)}
          placeholder={t("lfsLocks.pathPlaceholder")}
          className="h-7 flex-1 rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none"
        />
        <Button
          size="xs"
          variant="outline"
          disabled={!lockPath.trim()}
          onClick={() => {
            const p = lockPath.trim();
            setLockPath("");
            void lockFile(repoId, p);
          }}
        >
          <Lock />
          {t("lfsLocks.lock")}
        </Button>
        <Button size="xs" variant="ghost" onClick={refresh} title={t("lfsFiles.refresh")}>
          {q.isFetching ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
        </Button>
      </div>
      {q.isPending && <Note>{t("lfsLocks.loading")}</Note>}
      {q.isError && <Note tone="danger">{errorMessage(q.error)}</Note>}
      {q.data?.status === "failed" && (
        <Note tone="danger">
          {t(`lfsLocks.failure.${q.data.failure.kind}`, { path: "" })}{" "}
          {t(`lfsLocks.help.${q.data.failure.kind}`)}
        </Note>
      )}
      {q.data?.status === "done" && (
        <>
          {!q.data.value.verified && <Note>{t("lfsLocks.unverified")}</Note>}
          {q.data.value.locks.length === 0 && <Note>{t("lfsLocks.none")}</Note>}
          <ul className="divide-y rounded-md border">
            {q.data.value.locks.map((l) => (
              <li key={l.id || l.path} className="flex h-7 items-center gap-2 px-2 text-xs">
                <Lock className={cn("size-3.5 shrink-0", l.ours ? "text-info" : "text-warning")} />
                <span className="min-w-0 flex-1 truncate font-mono" title={l.path}>
                  {l.path}
                </span>
                <LockChip lock={l} />
                <span
                  className="w-36 shrink-0 truncate text-muted-foreground"
                  title={l.lockedAt ?? ""}
                >
                  {l.lockedAt ? new Date(l.lockedAt).toLocaleString() : ""}
                </span>
                {l.ours ? (
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() =>
                      void unlockFile(repoId, { path: l.path, id: l.id || null }, false)
                    }
                  >
                    <LockOpen />
                    {t("lfsLocks.unlock")}
                  </Button>
                ) : (
                  <Button
                    size="xs"
                    variant="ghost"
                    className="text-danger"
                    onClick={() => setForceUnlock(l)}
                  >
                    <LockOpen />
                    {t("lfsLocks.forceUnlock")}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
      <ForceUnlockDialog
        lock={forceUnlock}
        onConfirm={(lock) => {
          setForceUnlock(null);
          void unlockFile(repoId, { path: lock.path, id: lock.id || null }, true);
        }}
        onCancel={() => setForceUnlock(null)}
      />
    </div>
  );
}

// ----- patterns ------------------------------------------------------------------------------

function PatternsTab({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const status = useLfsStatus(repoId);
  const [trackOpen, setTrackOpen] = useState(false);
  const [tracking, setTracking] = useState(false);
  const [untracking, setUntracking] = useState<LfsPattern | null>(null);
  const s = status.data;
  if (!s) return <Note>{t("lfs.loading")}</Note>;
  return (
    <div className="grid gap-2">
      <p className="text-xs text-muted-foreground">{t("lfsLocks.lockableHint")}</p>
      {s.patterns.length === 0 && <Note>{t("lfs.noPatterns")}</Note>}
      <ul className="divide-y rounded-md border">
        {s.patterns.map((p) => (
          <li
            key={`${p.file}:${p.line}`}
            className="flex h-8 items-center gap-2 px-2 text-xs"
            title={t("lfs.declaredIn", { file: p.file, line: p.line })}
          >
            <FileType className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate font-mono">{p.pattern}</span>
            <span className="shrink-0 text-[10px] text-muted-foreground">{p.file}</span>
            <label className="flex shrink-0 items-center gap-1">
              <input
                type="checkbox"
                className="accent-primary"
                checked={p.lockable}
                disabled={!canUntrack(p)}
                onChange={(e) => void setLockable(repoId, p.pattern, e.target.checked)}
              />
              <Lock className="size-3" />
              {t("lfs.lockable")}
            </label>
            <Button
              size="xs"
              variant="ghost"
              disabled={!canUntrack(p)}
              title={canUntrack(p) ? t("lfs.untrack") : t("lfs.nestedOnly", { file: p.file })}
              onClick={() => setUntracking(p)}
            >
              <Unlink />
            </Button>
          </li>
        ))}
      </ul>
      <div>
        <Button size="xs" variant="outline" onClick={() => setTrackOpen(true)}>
          <Plus />
          {t("lfs.track")}
        </Button>
      </div>
      <TrackPatternDialog
        key={trackOpen ? "open" : "closed"}
        open={trackOpen}
        busy={tracking}
        onSubmit={(pattern, lockable) => {
          setTracking(true);
          void trackPattern(repoId, pattern, lockable).then((ok) => {
            setTracking(false);
            if (ok) setTrackOpen(false);
          });
        }}
        onCancel={() => setTrackOpen(false)}
      />
      <ConfirmDialog
        open={untracking !== null}
        title={t("lfs.untrackTitle", { pattern: untracking?.pattern ?? "" })}
        description={t("lfs.untrackBody")}
        confirmLabel={t("lfs.untrack")}
        destructive
        onConfirm={() => {
          const target = untracking;
          setUntracking(null);
          if (target) void untrackPattern(repoId, target.pattern);
        }}
        onCancel={() => setUntracking(null)}
      />
    </div>
  );
}

// ----- maintenance ---------------------------------------------------------------------------

function MaintenanceTab({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const storage = useLfsStorage(repoId);
  const refs = useRepoRefs(repoId);
  const [verifyRemote, setVerifyRemote] = useState(false);
  const [preview, setPreview] = useState<PruneReport | null>(null);
  const [pruneConfirm, setPruneConfirm] = useState(false);
  const [fsck, setFsck] = useState<FsckReport | null>(null);
  const [busy, setBusy] = useState<"preview" | "prune" | "fsck" | "guard" | null>(null);
  const notify = useUi((s) => s.showNotice);

  const run = async <T,>(kind: NonNullable<typeof busy>, f: () => Promise<T>) => {
    setBusy(kind);
    try {
      return await f();
    } catch (e) {
      notify({ kind: "error", message: errorMessage(e) });
      return undefined;
    } finally {
      setBusy(null);
    }
  };

  const doPreview = async () => {
    const r = await run("preview", () => commands.lfsPrunePreview(repoId, verifyRemote));
    if (r) setPreview(r);
  };
  const doPrune = async () => {
    setPruneConfirm(false);
    const r = await run("prune", () => commands.lfsPrune(repoId, verifyRemote));
    if (r) {
      notify({
        kind: "info",
        message: t("lfs.pruned", { count: r.objects.length, size: formatBytes(r.bytes) }),
      });
      setPreview(null);
      void queryClient.invalidateQueries({ queryKey: lfsKeys.storage(repoId) });
      invalidateLfs(repoId);
    }
  };
  const doFsck = async () => {
    const r = await run("fsck", () => commands.lfsFsck(repoId, true, true));
    if (r) setFsck(r);
  };

  const head = refs.data?.local.find((b) => b.isHead);
  const checkUnpushed = async () => {
    if (!head) return;
    const remote = head.upstream?.split("/")[0] ?? "origin";
    const out = await run("guard", () => commands.pushGuard(repoId, remote, head.name));
    if (!out) return;
    if (out.status !== "done") {
      notify({ kind: "error", message: t("lfs.guardFailed") });
      return;
    }
    if (!pushNeedsReview(out.value)) {
      notify({ kind: "info", message: t("lfs.guardClean") });
      return;
    }
    useGuardDialogs.getState().openPush({
      repoId,
      remote,
      branch: head.name,
      report: out.value,
      rewrite: null,
      resolve: () => undefined,
    });
  };

  const groups = fsck ? groupProblems(fsck.problems) : null;

  return (
    <div className="grid gap-4 text-xs">
      <section className="grid gap-1.5">
        <h4 className="flex items-center gap-1.5 font-semibold">
          <HardDrive className="size-3.5" />
          {t("lfs.storageTitle")}
        </h4>
        {storage.data ? (
          <p>
            {t("lfs.storage", {
              count: storage.data.objects,
              size: formatBytes(storage.data.bytes),
            })}
            {storage.data.mediaDir && (
              <span className="ml-1 font-mono text-[11px] text-muted-foreground">
                ({storage.data.mediaDir})
              </span>
            )}
          </p>
        ) : (
          <p className="text-muted-foreground">{t("lfs.loading")}</p>
        )}
        <div className="flex flex-wrap gap-2">
          <Button size="xs" variant="outline" onClick={() => void runLfsFetchAll(repoId)}>
            <Download />
            {t("lfs.fetchAll")}
          </Button>
        </div>
        <p className="text-[11px] text-muted-foreground">{t("lfs.fetchAllHint")}</p>
      </section>

      <section className="grid gap-1.5">
        <h4 className="flex items-center gap-1.5 font-semibold">
          <Trash2 className="size-3.5" />
          {t("lfs.pruneTitle")}
        </h4>
        <p className="text-muted-foreground">{t("lfs.pruneHint")}</p>
        <label className="flex items-center gap-1.5">
          <input
            type="checkbox"
            className="accent-primary"
            checked={verifyRemote}
            onChange={(e) => {
              setVerifyRemote(e.target.checked);
              setPreview(null);
            }}
          />
          {t("lfs.verifyRemote")}
        </label>
        <div className="flex gap-2">
          <Button
            size="xs"
            variant="outline"
            disabled={busy !== null}
            onClick={() => void doPreview()}
          >
            {busy === "preview" && <LoaderCircle className="animate-spin" />}
            {t("lfs.prunePreview")}
          </Button>
          <Button
            size="xs"
            variant="destructive"
            disabled={busy !== null || !preview || preview.objects.length === 0}
            onClick={() => setPruneConfirm(true)}
          >
            {busy === "prune" && <LoaderCircle className="animate-spin" />}
            {t("lfs.prune")}
          </Button>
        </div>
        {preview && (
          <Note tone={preview.objects.length ? "muted" : "ok"}>
            {preview.objects.length
              ? t(preview.approximate ? "lfs.pruneWouldApprox" : "lfs.pruneWould", {
                  count: preview.objects.length,
                  size: formatBytes(preview.bytes),
                  kept: preview.retained,
                })
              : t("lfs.pruneNothing", { kept: preview.retained })}
          </Note>
        )}
      </section>

      <section className="grid gap-1.5">
        <h4 className="flex items-center gap-1.5 font-semibold">
          <ShieldCheck className="size-3.5" />
          {t("lfs.fsckTitle")}
        </h4>
        <p className="text-muted-foreground">{t("lfs.fsckHint")}</p>
        <div>
          <Button
            size="xs"
            variant="outline"
            disabled={busy !== null}
            onClick={() => void doFsck()}
          >
            {busy === "fsck" && <LoaderCircle className="animate-spin" />}
            {t("lfs.fsck")}
          </Button>
        </div>
        {fsck && fsck.ok && <Note tone="ok">{t("lfs.fsckOk")}</Note>}
        {groups &&
          [...groups.entries()].map(([kind, list]) => (
            <div key={kind} className="grid gap-1 rounded-md border border-danger/40 p-2">
              <p className="flex items-center gap-1 font-semibold text-danger">
                <ShieldAlert className="size-3.5" />
                {t(`lfs.fsckKind.${kind}`, { count: list.length })}
              </p>
              <p className="text-muted-foreground">{t(`lfs.fsckHelp.${kind}`)}</p>
              <ul className="max-h-32 overflow-auto font-mono text-[11px]">
                {list.map((p) => (
                  <li key={p.detail} className="truncate" title={p.detail}>
                    {p.path ?? p.oid ?? p.detail}
                  </li>
                ))}
              </ul>
            </div>
          ))}
      </section>

      <section className="grid gap-1.5">
        <h4 className="font-semibold">{t("lfs.migrateTitle")}</h4>
        <p className="text-muted-foreground">{t("lfs.migrateHint")}</p>
        <div>
          <Button
            size="xs"
            variant="outline"
            disabled={!head || busy !== null}
            onClick={() => void checkUnpushed()}
          >
            {busy === "guard" && <LoaderCircle className="animate-spin" />}
            {t("lfs.migrateCheck")}
          </Button>
        </div>
      </section>

      <ConfirmDialog
        open={pruneConfirm}
        title={t("lfs.pruneConfirmTitle")}
        description={t("lfs.pruneConfirmBody", {
          count: preview?.objects.length ?? 0,
          size: formatBytes(preview?.bytes ?? 0),
        })}
        confirmLabel={t("lfs.prune")}
        destructive
        onConfirm={() => void doPrune()}
        onCancel={() => setPruneConfirm(false)}
      />
    </div>
  );
}
