import { lazy, Suspense, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { CircleHelp, GitMerge, LoaderCircle, RotateCcw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { splitPath } from "@/features/commit/format";
import { commands, errorMessage, type ConflictFile } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { closeFileView } from "@/stores/fileView";
import { conflictKeys, takeSide } from "./actions";
import { sideLabels, type SideLabels } from "./labels";
import { WholeFileResolver } from "./WholeFileResolver";

const ConflictEditor = lazy(() => import("./ConflictEditor"));

/** Center view for one conflicted file. */
export function ConflictView({ repoId, path }: { repoId: number; path: string }) {
  const { t } = useTranslation();
  const [explain, setExplain] = useState(false);
  // Loaded once per file: a background refetch would replace what the user is editing.
  const file = useQuery({
    queryKey: conflictKeys.file(repoId, path),
    queryFn: () => commands.conflictFile(repoId, path),
    enabled: isTauri(),
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  const context = useQuery({
    queryKey: conflictKeys.context(repoId),
    queryFn: () => commands.conflictContext(repoId),
    enabled: isTauri(),
    staleTime: 5_000,
  });
  const labels = sideLabels(context.data, t);
  const f = file.data;
  const { dir, name } = splitPath(path);
  const close = () => closeFileView(repoId);
  const reload = () => void queryClient.resetQueries({ queryKey: conflictKeys.file(repoId, path) });

  return (
    <div
      className="flex min-w-0 flex-1 flex-col bg-background outline-none"
      aria-label={t("conflict.title")}
      data-testid="conflict-view"
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.defaultPrevented && e.target === e.currentTarget) close();
      }}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <GitMerge className="size-4 shrink-0 text-danger" />
        <span className="min-w-24 flex-1 truncate font-medium" title={path}>
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </span>
        {f && !f.resolved && (
          <span className="shrink-0 rounded bg-danger/10 px-1.5 py-0.5 text-[11px] text-danger">
            {t(`conflict.kind.${f.kind}`, { ours: labels.oursShort, theirs: labels.theirsShort })}
          </span>
        )}
        <button
          type="button"
          onClick={() => setExplain(!explain)}
          aria-expanded={explain}
          className="flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <CircleHelp className="size-3.5" />
          {t("conflict.whatIsThis")}
        </button>
        {f && !f.resolved && f.editable && (
          <>
            <Button
              variant="outline"
              size="xs"
              onClick={() => void takeSide(repoId, path, "ours")}
              title={t("conflict.takeFileTitle")}
            >
              {f.ours
                ? t("conflict.takeFile", { side: labels.oursShort })
                : t("conflict.deleteFile")}
            </Button>
            <Button
              variant="outline"
              size="xs"
              onClick={() => void takeSide(repoId, path, "theirs")}
              title={t("conflict.takeFileTitle")}
            >
              {f.theirs
                ? t("conflict.takeFile", { side: labels.theirsShort })
                : t("conflict.deleteFile")}
            </Button>
          </>
        )}
        <button
          type="button"
          onClick={reload}
          title={t("conflict.reload")}
          aria-label={t("conflict.reload")}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <RotateCcw className="size-3.5" />
        </button>
        <button
          type="button"
          onClick={close}
          title={t("conflict.close")}
          aria-label={t("conflict.close")}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-4" />
        </button>
      </div>
      {explain && (
        <p className="shrink-0 border-b bg-info/10 px-3 py-2 text-xs leading-relaxed" role="note">
          {labels.explain}
        </p>
      )}
      <Body repoId={repoId} query={file} labels={labels} />
    </div>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}

function Loading() {
  const { t } = useTranslation();
  return (
    <Centered>
      <span className="flex items-center gap-2">
        <LoaderCircle className="size-4 animate-spin" />
        {t("conflict.loading")}
      </span>
    </Centered>
  );
}

function Body({
  repoId,
  query,
  labels,
}: {
  repoId: number;
  query: { data?: ConflictFile; isError: boolean; error: unknown };
  labels: SideLabels;
}) {
  const { t } = useTranslation();
  const f = query.data;
  if (query.isError)
    return <Centered>{t("conflict.error", { message: errorMessage(query.error) })}</Centered>;
  if (!f) return <Loading />;
  if (f.resolved) {
    return (
      <Centered>
        <p>{t("conflict.alreadyResolved")}</p>
        <Button size="xs" variant="outline" onClick={() => closeFileView(repoId)}>
          {t("conflict.close")}
        </Button>
      </Centered>
    );
  }
  const wholeFile =
    !f.editable ||
    f.kind === "deletedByUs" ||
    f.kind === "deletedByThem" ||
    f.kind === "addedByUs" ||
    f.kind === "addedByThem" ||
    f.kind === "bothDeleted";
  if (wholeFile) return <WholeFileResolver repoId={repoId} file={f} labels={labels} />;
  return (
    <Suspense fallback={<Loading />}>
      <ConflictEditor repoId={repoId} file={f} labels={labels} />
    </Suspense>
  );
}
