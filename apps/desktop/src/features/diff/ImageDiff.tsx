import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { CloudDownload, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { downloadLfsFile } from "@/features/transfers/actions";
import { commands, errorMessage, type BlobPreview, type DiffRequest } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  CHECKERBOARD,
  availableModes,
  clampPercent,
  dataUrl,
  dimensions,
  effectiveMode,
  swipeClip,
  type ImageMode,
} from "./imageModes";

type Size = { w: number; h: number } | null;

const imageKeys = {
  pair: (repoId: number, r: DiffRequest) =>
    ["contents", repoId, "images", r.side, r.path, r.oldPath ?? null, r.commit ?? null] as const,
};

/** Size, pixel dimensions and LFS pointer of one side. */
export function PreviewMeta({
  preview,
  size,
  className,
}: {
  preview: BlobPreview;
  size: Size;
  className?: string;
}) {
  const { t } = useTranslation();
  const dims = dimensions(size);
  return (
    <div className={cn("space-y-0.5 text-[11px] text-muted-foreground", className)}>
      <div className="tabular-nums">
        {dims && <span className="mr-2">{t("imageDiff.pixels", { size: dims })}</span>}
        <span>{formatBytes(preview.size)}</span>
      </div>
      {preview.lfs && (
        <div className="truncate font-mono" title={`sha256:${preview.lfs.oid}`}>
          {t("imageDiff.lfsPointer", {
            oid: preview.lfs.oid.slice(0, 12),
            size: formatBytes(preview.lfs.size),
          })}
        </div>
      )}
    </div>
  );
}

/** Why there is no picture to show, with a download button for a missing LFS object. */
export function PreviewProblem({
  preview,
  repoId,
  path,
}: {
  preview: BlobPreview;
  repoId?: number;
  path?: string;
}) {
  const { t } = useTranslation();
  if (preview.lfsMissing) {
    return (
      <div className="flex flex-col items-center gap-2 text-center text-xs text-muted-foreground">
        <p className="max-w-64">{t("imageDiff.lfsMissing")}</p>
        {repoId !== undefined && path && (
          <Button size="xs" variant="outline" onClick={() => void downloadLfsFile(repoId, path)}>
            <CloudDownload />
            {t("imageDiff.download")}
          </Button>
        )}
      </div>
    );
  }
  if (preview.tooLarge) {
    return (
      <p className="max-w-64 text-center text-xs text-muted-foreground">
        {t("imageDiff.tooLarge", { size: formatBytes(preview.size) })}
      </p>
    );
  }
  return <p className="text-xs text-muted-foreground">{t("imageDiff.noPreview")}</p>;
}

/** One picture on a checkerboard, reporting its natural size. */
export function PreviewImage({
  preview,
  alt,
  onSize,
  className,
}: {
  preview: BlobPreview;
  alt: string;
  onSize?: (size: Size) => void;
  className?: string;
}) {
  const url = dataUrl(preview);
  const [broken, setBroken] = useState(false);
  const { t } = useTranslation();
  if (!url) return null;
  if (broken) return <p className="text-xs text-muted-foreground">{t("imageDiff.cannotDecode")}</p>;
  return (
    <img
      src={url}
      alt={alt}
      draggable={false}
      onLoad={(e) =>
        onSize?.({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
      }
      onError={() => setBroken(true)}
      className={cn("max-h-full max-w-full object-contain", className)}
      style={CHECKERBOARD}
    />
  );
}

function Side({
  label,
  preview,
  missingText,
  repoId,
  path,
  onSize,
  size,
  tone,
}: {
  label: string;
  preview: BlobPreview | null;
  missingText: string;
  repoId: number;
  path: string;
  onSize: (s: Size) => void;
  size: Size;
  tone: "old" | "new";
}) {
  return (
    <figure className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 p-3">
      <figcaption className="flex items-center gap-2 text-xs font-medium">
        <span
          className={cn("size-2 rounded-full", tone === "old" ? "bg-danger" : "bg-success")}
          aria-hidden
        />
        {label}
      </figcaption>
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto rounded border bg-background p-2">
        {!preview ? (
          <p className="text-xs text-muted-foreground">{missingText}</p>
        ) : dataUrl(preview) ? (
          <PreviewImage preview={preview} alt={label} onSize={onSize} />
        ) : (
          <PreviewProblem preview={preview} repoId={repoId} path={path} />
        )}
      </div>
      {preview && <PreviewMeta preview={preview} size={size} />}
    </figure>
  );
}

/** Image diff: side by side, swipe or onion skin. */
export function ImageDiff({ repoId, request }: { repoId: number; request: DiffRequest }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<ImageMode>("side");
  const [swipe, setSwipe] = useState(50);
  const [opacity, setOpacity] = useState(50);
  const [oldSize, setOldSize] = useState<Size>(null);
  const [newSize, setNewSize] = useState<Size>(null);
  const pair = useQuery({
    queryKey: imageKeys.pair(repoId, request),
    queryFn: () => commands.diffImages(repoId, request),
    enabled: isTauri(),
  });
  if (pair.isError) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-xs text-muted-foreground">
        {t("diff.error", { message: errorMessage(pair.error) })}
      </div>
    );
  }
  const p = pair.data;
  if (!p) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
        <LoaderCircle className="size-4 animate-spin" />
        {t("diff.loading")}
      </div>
    );
  }
  const modes = availableModes(p.original, p.modified);
  const shown = effectiveMode(mode, p.original, p.modified);
  const oldLabel = t("imageDiff.before");
  const newLabel = t("imageDiff.after");
  const oldUrl = dataUrl(p.original);
  const newUrl = dataUrl(p.modified);

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="image-diff">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          value={shown}
          onValueChange={(v) => v && setMode(v as ImageMode)}
        >
          {(["side", "swipe", "onion"] as const).map((m) => (
            <ToggleGroupItem
              key={m}
              value={m}
              disabled={!modes.includes(m)}
              className="h-6 px-2 text-[11px]"
            >
              {t(`imageDiff.mode.${m}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {shown === "swipe" && (
          <label className="flex items-center gap-2 text-[11px] text-muted-foreground">
            {t("imageDiff.swipePosition")}
            <input
              type="range"
              min={0}
              max={100}
              value={swipe}
              onChange={(e) => setSwipe(clampPercent(Number(e.target.value)))}
              aria-label={t("imageDiff.swipePosition")}
            />
          </label>
        )}
        {shown === "onion" && (
          <label className="flex items-center gap-2 text-[11px] text-muted-foreground">
            {t("imageDiff.opacity")}
            <input
              type="range"
              min={0}
              max={100}
              value={opacity}
              onChange={(e) => setOpacity(clampPercent(Number(e.target.value)))}
              aria-label={t("imageDiff.opacity")}
            />
            <span className="w-8 tabular-nums">{opacity}%</span>
          </label>
        )}
      </div>
      {shown === "side" ? (
        <div className="flex min-h-0 flex-1">
          <Side
            label={oldLabel}
            preview={p.original}
            missingText={t("imageDiff.noOld")}
            repoId={repoId}
            path={request.oldPath ?? request.path}
            onSize={setOldSize}
            size={oldSize}
            tone="old"
          />
          <div className="w-px bg-border" />
          <Side
            label={newLabel}
            preview={p.modified}
            missingText={t("imageDiff.noNew")}
            repoId={repoId}
            path={request.path}
            onSize={setNewSize}
            size={newSize}
            tone="new"
          />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
          <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto rounded border bg-background p-2">
            <div className="relative inline-grid" style={CHECKERBOARD}>
              {/* Both pictures in the same grid cell, scaled the same way. */}
              <img
                src={newUrl!}
                alt={newLabel}
                draggable={false}
                className="col-start-1 row-start-1 max-h-[70vh] max-w-full object-contain"
                onLoad={(e) =>
                  setNewSize({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
                }
              />
              <img
                src={oldUrl!}
                alt={oldLabel}
                draggable={false}
                data-testid={shown === "swipe" ? "swipe-top" : "onion-top"}
                className="col-start-1 row-start-1 h-full w-full object-contain"
                style={
                  shown === "swipe"
                    ? { clipPath: swipeClip(swipe) }
                    : { opacity: (100 - opacity) / 100 }
                }
                onLoad={(e) =>
                  setOldSize({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
                }
              />
              {shown === "swipe" && (
                <div
                  className="pointer-events-none absolute inset-y-0 w-0.5 bg-primary"
                  style={{ left: `${swipe}%` }}
                />
              )}
            </div>
          </div>
          <div className="flex gap-6">
            <div>
              <span className="text-xs font-medium">{oldLabel}</span>
              <PreviewMeta preview={p.original!} size={oldSize} />
            </div>
            <div>
              <span className="text-xs font-medium">{newLabel}</span>
              <PreviewMeta preview={p.modified!} size={newSize} />
            </div>
            {shown === "swipe" && (
              <p className="ml-auto self-end text-[11px] text-muted-foreground">
                {t("imageDiff.swipeHint")}
              </p>
            )}
            {shown === "onion" && (
              <p className="ml-auto self-end text-[11px] text-muted-foreground">
                {t("imageDiff.onionHint")}
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
