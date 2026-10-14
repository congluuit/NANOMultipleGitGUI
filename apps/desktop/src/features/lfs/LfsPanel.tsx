import { useState, type ReactNode } from "react";
import { FileType, LoaderCircle, Lock, Plus, TriangleAlert, Unlink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { errorMessage, type LfsPattern } from "@/ipc";
import { cn } from "@/lib/utils";
import { installLfsHooks, trackPattern, untrackPattern } from "./actions";
import { canUntrack, lfsVersion, shouldOfferInstall } from "./patterns";
import { useLfsChangeEvents, useLfsStatus } from "./queries";
import { TrackPatternDialog } from "./TrackPatternDialog";

function Row({
  children,
  className,
  title,
}: {
  children: ReactNode;
  className?: string;
  title?: string;
}) {
  return (
    <li
      title={title}
      className={cn("flex min-h-6 items-center gap-2 py-0.5 pr-2 pl-8 text-xs", className)}
    >
      {children}
    </li>
  );
}

/** Content of the sidebar "LFS" section for one repository. */
export function LfsPanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const status = useLfsStatus(repoId);
  useLfsChangeEvents(repoId);
  const [trackOpen, setTrackOpen] = useState(false);
  const [tracking, setTracking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [untracking, setUntracking] = useState<LfsPattern | null>(null);
  const s = status.data;

  if (status.isPending) {
    return (
      <Row className="text-muted-foreground italic">
        {status.fetchStatus === "fetching" && <LoaderCircle className="size-3 animate-spin" />}
        {t("lfs.loading")}
      </Row>
    );
  }
  if (status.isError || !s) {
    return (
      <Row className="text-danger">{t("lfs.error", { message: errorMessage(status.error) })}</Row>
    );
  }

  const version = lfsVersion(s);
  const offerInstall = shouldOfferInstall(s);

  const submitTrack = async (pattern: string, lockable: boolean) => {
    setTracking(true);
    const ok = await trackPattern(repoId, pattern, lockable);
    setTracking(false);
    if (ok) setTrackOpen(false);
  };

  const install = async () => {
    setInstalling(true);
    await installLfsHooks(repoId);
    setInstalling(false);
  };

  const confirmUntrack = () => {
    const target = untracking;
    setUntracking(null);
    if (target) void untrackPattern(repoId, target.pattern);
  };

  return (
    <>
      <Row
        className={version ? "text-muted-foreground" : "text-warning"}
        title={s.gitLfs.raw ?? undefined}
      >
        <span className="shrink-0 whitespace-nowrap">
          {version ? t("lfs.version", { version }) : t("lfs.notInstalled")}
        </span>
        <span aria-hidden>·</span>
        <span className="truncate">{s.usesLfs ? t("lfs.usesLfs") : t("lfs.notUsed")}</span>
      </Row>
      {!version && s.usesLfs && (
        <Row className="text-warning">
          <TriangleAlert className="size-3.5 shrink-0" />
          <span>{t("lfs.missingTool")}</span>
        </Row>
      )}
      {offerInstall && (
        <li className="mx-2 my-1 space-y-1.5 rounded-md border border-warning/40 bg-warning/10 p-2 text-xs text-warning">
          <div className="flex items-start gap-1.5">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
            <span>{t("lfs.needsInstall")}</span>
          </div>
          <Button
            size="xs"
            variant="outline"
            className="w-full"
            disabled={installing}
            title={t("lfs.installHint")}
            onClick={() => void install()}
          >
            {installing && <LoaderCircle className="animate-spin" />}
            {t("lfs.install")}
          </Button>
        </li>
      )}
      <li className="pt-1 pr-2 pl-8 text-[11px] text-muted-foreground">
        {t("lfs.patterns")} ({s.patterns.length})
      </li>
      {s.patterns.length === 0 && (
        <Row className="text-[11px] text-muted-foreground italic">{t("lfs.noPatterns")}</Row>
      )}
      {s.patterns.map((p) => (
        <ContextMenu key={`${p.file}:${p.line}`}>
          <ContextMenuTrigger asChild>
            <li
              title={t("lfs.declaredIn", { file: p.file, line: p.line })}
              className="flex h-6 cursor-default items-center gap-2 pr-2 pl-8 text-xs hover:bg-surface-2 data-[state=open]:bg-surface-2"
            >
              <FileType className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="flex-1 truncate font-mono">{p.pattern}</span>
              {p.lockable && (
                <span className="flex shrink-0 items-center gap-0.5 rounded border px-1 text-[10px] text-muted-foreground">
                  <Lock className="size-2.5" />
                  {t("lfs.lockable")}
                </span>
              )}
            </li>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuLabel className="font-mono">{p.pattern}</ContextMenuLabel>
            <ContextMenuItem
              variant="destructive"
              disabled={!canUntrack(p)}
              onSelect={() => setUntracking(p)}
            >
              <Unlink />
              {t("lfs.untrack")}
            </ContextMenuItem>
            {!canUntrack(p) && (
              <ContextMenuLabel>{t("lfs.nestedOnly", { file: p.file })}</ContextMenuLabel>
            )}
          </ContextMenuContent>
        </ContextMenu>
      ))}
      <li className="px-2 pt-1">
        <Button
          size="xs"
          variant="ghost"
          className="w-full justify-start pl-6 text-muted-foreground"
          disabled={!version}
          onClick={() => setTrackOpen(true)}
        >
          <Plus />
          {t("lfs.track")}
        </Button>
      </li>
      <TrackPatternDialog
        key={trackOpen ? "open" : "closed"}
        open={trackOpen}
        busy={tracking}
        onSubmit={(pattern, lockable) => void submitTrack(pattern, lockable)}
        onCancel={() => setTrackOpen(false)}
      />
      <ConfirmDialog
        open={untracking !== null}
        title={t("lfs.untrackTitle", { pattern: untracking?.pattern ?? "" })}
        description={t("lfs.untrackBody")}
        confirmLabel={t("lfs.untrack")}
        destructive
        onConfirm={confirmUntrack}
        onCancel={() => setUntracking(null)}
      />
    </>
  );
}
