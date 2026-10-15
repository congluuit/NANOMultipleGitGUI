import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { Check, LoaderCircle, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { PreviewImage, PreviewMeta, PreviewProblem } from "@/features/diff/ImageDiff";
import { dataUrl } from "@/features/diff/imageModes";
import { commands, type ConflictFile, type ConflictSide, type ConflictStage } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { conflictKeys, deleteConflicted, takeSide } from "./actions";
import type { SideLabels } from "./labels";

const SNIPPET_LINES = 60;

type Size = { w: number; h: number } | null;

function StagePreview({
  repoId,
  path,
  stage,
  label,
}: {
  repoId: number;
  path: string;
  stage: 2 | 3;
  label: string;
}) {
  const { t } = useTranslation();
  const [size, setSize] = useState<Size>(null);
  const preview = useQuery({
    queryKey: conflictKeys.preview(repoId, path, stage),
    queryFn: () => commands.blobPreview(repoId, { kind: "index", stage }, path),
    enabled: isTauri(),
  });
  const p = preview.data;
  if (preview.isPending)
    return <LoaderCircle className="size-4 animate-spin text-muted-foreground" />;
  if (!p) return <p className="text-xs text-muted-foreground">{t("imageDiff.noPreview")}</p>;
  return (
    <div className="flex min-h-0 w-full flex-1 flex-col items-center gap-2">
      <div className="flex min-h-32 w-full flex-1 items-center justify-center overflow-auto rounded border bg-background p-2">
        {dataUrl(p) ? (
          <PreviewImage preview={p} alt={label} onSize={setSize} className="max-h-72" />
        ) : (
          <PreviewProblem preview={p} />
        )}
      </div>
      <PreviewMeta preview={p} size={size} className="self-start" />
    </div>
  );
}

function SideCard({
  repoId,
  file,
  side,
  stage,
  title,
  detail,
  short,
  onPick,
  busy,
}: {
  repoId: number;
  file: ConflictFile;
  side: ConflictSide;
  stage: ConflictStage | null;
  title: string;
  detail: string | null;
  short: string;
  onPick: () => void;
  busy: boolean;
}) {
  const { t } = useTranslation();
  const snippet = stage?.text?.split("\n").slice(0, SNIPPET_LINES).join("\n");
  return (
    <section
      className={cn(
        "flex min-h-0 min-w-0 flex-1 flex-col gap-2 rounded-lg border bg-surface p-3",
        side === "ours" ? "border-l-4 border-l-success" : "border-l-4 border-l-info",
      )}
      aria-label={title}
    >
      <header className="min-w-0">
        <h3 className="truncate text-xs font-semibold">{title}</h3>
        {detail && (
          <p className="truncate font-mono text-[11px] text-muted-foreground" title={detail}>
            {detail}
          </p>
        )}
      </header>
      {!stage ? (
        <p className="flex flex-1 items-center justify-center rounded border border-dashed p-4 text-center text-xs text-muted-foreground">
          {t("conflict.sideDeleted", { side: short })}
        </p>
      ) : file.image ? (
        <StagePreview
          repoId={repoId}
          path={file.path}
          stage={side === "ours" ? 2 : 3}
          label={title}
        />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          <p className="text-[11px] text-muted-foreground">
            {t("conflict.sideSize", { size: formatBytes(stage.lfs?.size ?? stage.size) })}
            {stage.lfs &&
              ` · ${t("imageDiff.lfsPointer", {
                oid: stage.lfs.oid.slice(0, 12),
                size: formatBytes(stage.lfs.size),
              })}`}
          </p>
          {snippet !== undefined ? (
            <pre className="min-h-0 flex-1 overflow-auto rounded border bg-background p-2 font-mono text-[11px] leading-snug">
              {snippet}
            </pre>
          ) : (
            <p className="flex flex-1 items-center justify-center rounded border border-dashed p-4 text-xs text-muted-foreground">
              {t("conflict.paneBinary")}
            </p>
          )}
        </div>
      )}
      <Button size="xs" disabled={busy} onClick={onPick} className="self-start">
        {stage ? <Check /> : <Trash2 />}
        {stage ? t("conflict.keepSide", { side: short }) : t("conflict.acceptDelete")}
      </Button>
    </section>
  );
}

/**
 * Resolve by keeping one side as a whole: binary / LFS files, modify/delete and other
 * conflicts that cannot be merged line by line.
 */
export function WholeFileResolver({
  repoId,
  file,
  labels,
}: {
  repoId: number;
  file: ConflictFile;
  labels: SideLabels;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const run = async (op: () => Promise<boolean>) => {
    setBusy(true);
    await op();
    setBusy(false);
  };
  const hint = file.binary
    ? t("conflict.binaryHint")
    : !file.editable
      ? t("conflict.notEditable")
      : t("conflict.wholeFileHint");
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
      <p className="text-xs text-muted-foreground">{hint}</p>
      <div className="flex min-h-0 flex-1 gap-3">
        <SideCard
          repoId={repoId}
          file={file}
          side="ours"
          stage={file.ours}
          title={labels.ours}
          detail={labels.oursDetail}
          short={labels.oursShort}
          busy={busy}
          onPick={() => void run(() => takeSide(repoId, file.path, "ours"))}
        />
        <SideCard
          repoId={repoId}
          file={file}
          side="theirs"
          stage={file.theirs}
          title={labels.theirs}
          detail={labels.theirsDetail}
          short={labels.theirsShort}
          busy={busy}
          onPick={() => void run(() => takeSide(repoId, file.path, "theirs"))}
        />
      </div>
      {file.ours && file.theirs && (
        <Button
          variant="outline"
          size="xs"
          disabled={busy}
          className="self-start"
          onClick={() => void run(() => deleteConflicted(repoId, file.path))}
        >
          <Trash2 />
          {t("conflict.deleteFile")}
        </Button>
      )}
    </div>
  );
}
