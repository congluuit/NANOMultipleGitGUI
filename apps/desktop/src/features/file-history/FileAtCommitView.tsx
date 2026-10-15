import { lazy, Suspense, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { Eye, GitCommitHorizontal, History, LoaderCircle, UserRound, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { splitPath } from "@/features/commit/format";
import { PreviewImage, PreviewMeta, PreviewProblem } from "@/features/diff/ImageDiff";
import { dataUrl } from "@/features/diff/imageModes";
import { commands, errorMessage } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { closeFileView, jumpToCommit, openFileView } from "@/stores/fileView";

const MonacoView = lazy(() => import("@/features/diff/MonacoView"));

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}

function ImageAtCommit({ repoId, path, commit }: { repoId: number; path: string; commit: string }) {
  const { t } = useTranslation();
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  const preview = useQuery({
    queryKey: ["contents", repoId, "blob", commit, path],
    queryFn: () => commands.blobPreview(repoId, { kind: "commit", oid: commit }, path),
    enabled: isTauri(),
    staleTime: Infinity,
  });
  const p = preview.data;
  if (preview.isPending)
    return (
      <Centered>
        <LoaderCircle className="size-4 animate-spin" />
      </Centered>
    );
  if (!p) return <Centered>{t("fileHistory.fileMissing")}</Centered>;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto rounded border p-2">
        {dataUrl(p) ? (
          <PreviewImage preview={p} alt={path} onSize={setSize} />
        ) : (
          <PreviewProblem preview={p} repoId={repoId} path={path} />
        )}
      </div>
      <PreviewMeta preview={p} size={size} />
    </div>
  );
}

/** Read-only view of a file as of one commit. */
export function FileAtCommitView({
  repoId,
  path,
  commit,
}: {
  repoId: number;
  path: string;
  commit: string;
}) {
  const { t } = useTranslation();
  const file = useQuery({
    queryKey: ["fileAt", repoId, commit, path],
    queryFn: () => commands.fileAtCommit(repoId, commit, path),
    enabled: isTauri(),
    staleTime: Infinity,
  });
  const f = file.data;
  const { dir, name } = splitPath(path);
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background" data-testid="file-at-commit">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <Eye className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-24 flex-1 truncate font-medium" title={path}>
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </span>
        <button
          type="button"
          onClick={() => jumpToCommit(repoId, commit)}
          className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[11px] text-primary hover:underline"
          title={t("fileHistory.showInGraph")}
        >
          <GitCommitHorizontal className="mr-1 inline size-3" />
          {commit.slice(0, 7)}
        </button>
        {f?.exists && (
          <span className="shrink-0 text-[11px] text-muted-foreground">{formatBytes(f.size)}</span>
        )}
        <Button
          variant="outline"
          size="xs"
          onClick={() => openFileView(repoId, { kind: "blame", path, rev: commit })}
        >
          <UserRound />
          {t("fileHistory.blameHere")}
        </Button>
        <Button
          variant="outline"
          size="xs"
          onClick={() => openFileView(repoId, { kind: "history", path })}
        >
          <History />
          {t("fileHistory.menuHistory")}
        </Button>
        <button
          type="button"
          onClick={() => closeFileView(repoId)}
          title={t("fileHistory.close")}
          aria-label={t("fileHistory.close")}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-4" />
        </button>
      </div>
      {file.isError ? (
        <Centered>{t("fileHistory.error", { message: errorMessage(file.error) })}</Centered>
      ) : !f ? (
        <Centered>
          <LoaderCircle className="size-4 animate-spin" />
        </Centered>
      ) : !f.exists ? (
        <Centered>{t("fileHistory.fileMissing")}</Centered>
      ) : f.image && (f.binary || f.lfs) ? (
        <ImageAtCommit repoId={repoId} path={path} commit={commit} />
      ) : f.tooLarge ? (
        <Centered>{t("fileHistory.fileTooLarge", { size: formatBytes(f.size) })}</Centered>
      ) : f.binary || f.text === null ? (
        <Centered>{t("fileHistory.fileBinary")}</Centered>
      ) : (
        <>
          {f.lfs && (
            <p className="shrink-0 border-b bg-info/10 px-3 py-1.5 text-[11px]">
              {t("fileHistory.lfsPointerNote", {
                oid: f.lfs.oid.slice(0, 12),
                size: formatBytes(f.lfs.size),
              })}
            </p>
          )}
          <Suspense
            fallback={
              <Centered>
                <LoaderCircle className="size-4 animate-spin" />
              </Centered>
            }
          >
            <MonacoView path={path} value={f.text} />
          </Suspense>
        </>
      )}
    </div>
  );
}
