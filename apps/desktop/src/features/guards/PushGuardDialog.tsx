import {
  ArchiveRestore,
  FileWarning,
  Info,
  LoaderCircle,
  OctagonX,
  TriangleAlert,
  UserPen,
} from "lucide-react";
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
import type { FixAuthorMode, GuardHeavyBlob, PushGuardReport, RewriteOutcome } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  canMigrate,
  canPushAnyway,
  fixAvailability,
  migratableBlobs,
  migratePaths,
  permissionDenied,
  sharedBlobs,
  type FixBlocker,
} from "./model";

interface Props {
  /** `null` = closed. */
  report: PushGuardReport | null;
  /** Last rewrite done from this dialog (backup ref + Undo). */
  rewrite?: RewriteOutcome | null;
  busy?: boolean;
  onFixAuthors: (mode: FixAuthorMode) => void;
  onMigrate: (paths: string[]) => void;
  onUndo: (backupRef: string) => void;
  onPush: () => void;
  onCancel: () => void;
}

function Section({
  tone,
  icon,
  title,
  children,
}: {
  tone: "warning" | "danger" | "info";
  icon: React.ReactNode;
  title: string;
  children?: React.ReactNode;
}) {
  const toneClass = {
    warning: "border-warning/40 bg-warning/10",
    danger: "border-danger/40 bg-danger/10",
    info: "border-info/40 bg-info/10",
  }[tone];
  const textClass = { warning: "text-warning", danger: "text-danger", info: "text-info" }[tone];
  return (
    <section
      role={tone === "info" ? "status" : "alert"}
      className={cn("space-y-1.5 rounded-md border p-2 text-xs", toneClass)}
    >
      <div className={cn("flex items-start gap-2 font-medium", textClass)}>
        <span className="mt-0.5 shrink-0">{icon}</span>
        <span className="min-w-0">{title}</span>
      </div>
      {children}
    </section>
  );
}

function BlobRow({ blob }: { blob: GuardHeavyBlob }) {
  const { t } = useTranslation();
  return (
    <li className="flex items-center gap-2 px-2 py-1">
      <span className="min-w-0 flex-1 truncate font-mono" title={blob.path}>
        {blob.path}
      </span>
      <span
        className={cn(
          "shrink-0 rounded border px-1 text-[10px] leading-4",
          blob.level === "block"
            ? "border-danger/40 text-danger"
            : "border-warning/40 text-warning",
        )}
      >
        {blob.level === "block" ? t("guards.push.levelBlock") : t("guards.push.levelWarn")}
      </span>
      {blob.commits.length > 0 && (
        <span className="shrink-0 text-muted-foreground">
          {t("guards.push.inCommits", { count: blob.commits.length })}
        </span>
      )}
      <span className="w-16 shrink-0 text-right font-mono tabular-nums">
        {formatBytes(blob.size)}
      </span>
    </li>
  );
}

/**
 * Push Guard (PLAN §4.6, §6.3): mismatched authors with [Sửa tác giả], permission block,
 * heavy blobs with [Chuyển sang LFS]; every rewrite shows its backup ref and an Undo.
 */
export function PushGuardDialog({
  report,
  rewrite = null,
  busy,
  onFixAuthors,
  onMigrate,
  onUndo,
  onPush,
  onCancel,
}: Props) {
  const { t } = useTranslation();
  const r = report;
  const denied = r ? permissionDenied(r) : null;
  const migratable = r ? migratableBlobs(r) : [];
  const shared = r ? sharedBlobs(r) : [];
  const fixLast = r ? fixAvailability(r, "amendLast") : null;
  const fixAll = r ? fixAvailability(r, "rewriteUnpushed") : null;
  const anyway = r ? canPushAnyway(r) : false;
  const clean = r !== null && r.findings.length === 0;
  const blockerText = (reason: FixBlocker) =>
    reason === "notCurrent"
      ? t("guards.push.fixNotCurrent", { branch: r?.branch ?? "" })
      : reason === "shared"
        ? t("guards.push.fixShared")
        : reason === "lastFine"
          ? t("guards.push.fixLastUnavailable")
          : undefined;
  const thresholds = r?.thresholds;

  return (
    <Dialog open={r !== null} onOpenChange={(o) => !o && !busy && onCancel()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <TriangleAlert className="size-5 shrink-0 text-warning" />
            {t("guards.push.title")}
          </DialogTitle>
          <DialogDescription>
            {r &&
              t("guards.push.summary", {
                count: r.totalUnpushed,
                remote: r.remote,
                branch: r.branch,
              })}
            {r && r.upstream === null && <> {t("guards.push.newBranch")}</>}
            {r?.identity && (
              <span className="block break-all">
                {t("guards.push.identity", { email: r.identity.email })}
              </span>
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[55vh] space-y-2 overflow-y-auto">
          {r && denied?.kind === "permissionDenied" && (
            <Section
              tone="danger"
              icon={<OctagonX className="size-3.5" />}
              title={
                denied.accountLabel
                  ? t("guards.push.permissionDenied", {
                      label: denied.accountLabel,
                      level: denied.level ?? "none",
                    })
                  : t("guards.push.permissionDeniedNoLabel")
              }
            />
          )}

          {r && r.authorMismatch.length > 0 && (
            <Section
              tone="warning"
              icon={<UserPen className="size-3.5" />}
              title={t("guards.push.authorsTitle", { count: r.authorMismatch.length })}
            >
              {r.identity && <p>{t("guards.push.authorsDesc", { email: r.identity.email })}</p>}
              <ul className="max-h-40 divide-y overflow-y-auto rounded border bg-background">
                {r.authorMismatch.map((c) => (
                  <li key={c.oid} className="flex items-center gap-2 px-2 py-1">
                    <span className="shrink-0 font-mono text-muted-foreground">{c.shortOid}</span>
                    <span className="min-w-0 flex-1 truncate" title={c.summary}>
                      {c.summary}
                    </span>
                    {c.shared && (
                      <span className="shrink-0 rounded border px-1 text-[10px] leading-4">
                        {t("guards.push.sharedCommit")}
                      </span>
                    )}
                    <span className="max-w-[45%] shrink-0 truncate font-mono" title={c.authorEmail}>
                      {c.authorEmail}
                    </span>
                  </li>
                ))}
              </ul>
              <div className="flex flex-wrap gap-2">
                {fixLast && (
                  <span title={fixLast.available ? undefined : blockerText(fixLast.reason)}>
                    <Button
                      size="xs"
                      variant="outline"
                      disabled={busy || !fixLast.available}
                      onClick={() => onFixAuthors("amendLast")}
                    >
                      {t("guards.push.fixLast")}
                    </Button>
                  </span>
                )}
                {fixAll && (
                  <span title={fixAll.available ? undefined : blockerText(fixAll.reason)}>
                    <Button
                      size="xs"
                      variant="outline"
                      disabled={busy || !fixAll.available}
                      onClick={() => onFixAuthors("rewriteUnpushed")}
                    >
                      {t("guards.push.fixAll", { count: r.totalUnpushed })}
                    </Button>
                  </span>
                )}
              </div>
            </Section>
          )}

          {r && migratable.length > 0 && thresholds && (
            <Section
              tone={migratable.some((b) => b.level === "block") ? "danger" : "warning"}
              icon={<FileWarning className="size-3.5" />}
              title={t("guards.push.blobsTitle", { count: migratable.length })}
            >
              <p>
                {t("guards.push.blobsDesc", {
                  warn: formatBytes(thresholds.warnBytes),
                  block:
                    thresholds.blockBytes !== null
                      ? t("guards.push.blockSuffix", { size: formatBytes(thresholds.blockBytes) })
                      : "",
                })}
              </p>
              <ul className="max-h-40 divide-y overflow-y-auto rounded border bg-background">
                {migratable.map((b) => (
                  <BlobRow key={b.oid} blob={b} />
                ))}
              </ul>
              <div className="flex flex-wrap items-center gap-2">
                <span
                  title={
                    !r.isCurrent
                      ? t("guards.push.fixNotCurrent", { branch: r.branch })
                      : !r.rewritable
                        ? t("guards.push.fixShared")
                        : t("guards.push.migrateDesc", { paths: migratePaths(r).join(", ") })
                  }
                >
                  <Button
                    size="xs"
                    disabled={busy || !canMigrate(r)}
                    onClick={() => onMigrate(migratePaths(r))}
                  >
                    {t("guards.push.migrate")}
                  </Button>
                </span>
              </div>
            </Section>
          )}

          {shared.length > 0 && (
            <Section
              tone="info"
              icon={<Info className="size-3.5" />}
              title={t("guards.push.sharedBlobsTitle")}
            >
              <ul className="divide-y rounded border bg-background">
                {shared.map((b) => (
                  <BlobRow key={b.oid} blob={b} />
                ))}
              </ul>
              <p>{t("guards.push.sharedBlobsDesc")}</p>
            </Section>
          )}

          {rewrite && (
            <Section
              tone="info"
              icon={<ArchiveRestore className="size-3.5" />}
              title={t("guards.push.backupDone", {
                count: rewrite.rewritten,
                ref: rewrite.backupRef,
              })}
            >
              <Button
                size="xs"
                variant="outline"
                disabled={busy}
                onClick={() => onUndo(rewrite.backupRef)}
              >
                {t("guards.push.undo")}
              </Button>
            </Section>
          )}
        </div>

        <DialogFooter className="sm:justify-between">
          <Button variant="outline" size="sm" disabled={busy} onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <span title={anyway ? undefined : t("guards.push.pushBlocked")}>
            <Button
              size="sm"
              variant={clean ? "default" : "outline"}
              className="w-full"
              disabled={busy || !anyway}
              onClick={onPush}
            >
              {busy && <LoaderCircle className="animate-spin" />}
              {clean ? t("guards.push.push") : t("guards.push.pushAnyway")}
            </Button>
          </span>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
