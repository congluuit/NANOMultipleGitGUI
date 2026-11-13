import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { LoaderCircle, TriangleAlert } from "lucide-react";
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
import { splitPath } from "@/features/commit/format";
import { commands, errorMessage, type ApplyMode, type PatchInfo } from "@/ipc";
import { applyPatch } from "./actions";

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

function PatchPreview({ info }: { info: PatchInfo }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      {info.subjects.length > 0 && (
        <section>
          <h3 className="mb-1 text-[11px] font-semibold text-muted-foreground uppercase">
            {t("patch.commits", { count: info.commits })}
          </h3>
          <ul className="max-h-24 overflow-y-auto rounded-md border bg-background py-1">
            {info.subjects.map((s, i) => (
              <li key={`${i}:${s}`} className="truncate px-2 py-0.5 text-xs">
                {s}
              </li>
            ))}
          </ul>
        </section>
      )}
      <section>
        <h3 className="mb-1 text-[11px] font-semibold text-muted-foreground uppercase">
          {t("patch.files", { count: info.files.length })}
        </h3>
        <ul className="max-h-[36vh] overflow-y-auto rounded-md border bg-background py-1">
          {info.files.map((f) => {
            const { dir, name } = splitPath(f.path);
            return (
              <li key={f.path} className="flex items-center gap-2 px-2 py-0.5 text-xs">
                <span className="min-w-0 flex-1 truncate" title={f.path}>
                  <span className="text-muted-foreground">{dir}</span>
                  {name}
                </span>
                {f.binary ? (
                  <span className="shrink-0 text-[11px] text-muted-foreground">
                    {t("stash.binary")}
                  </span>
                ) : (
                  <span className="shrink-0 font-mono text-[11px] tabular-nums">
                    <span className="text-success">+{f.added}</span>{" "}
                    <span className="text-danger">-{f.removed}</span>
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      </section>
      {!info.applies && (
        <p className="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-xs">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" />
          <span>{t("patch.notClean")}</span>
        </p>
      )}
    </div>
  );
}

/** What a patch file contains, then "Apply as changes" or "Apply as commits". */
export function PatchApplyDialog({
  repoId,
  file,
  onClose,
}: {
  repoId: number;
  file: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState<ApplyMode | null>(null);
  const inspect = useQuery({
    queryKey: ["patchInspect", repoId, file],
    queryFn: () => commands.patchInspect(repoId, file),
    enabled: isTauri(),
    gcTime: 0,
  });
  const out = inspect.data;
  const info = out?.status === "done" ? out.value : null;

  const run = async (mode: ApplyMode) => {
    setBusy(mode);
    const ok = await applyPatch(repoId, file, mode);
    setBusy(null);
    if (ok) onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("patch.applyTitle")}</DialogTitle>
          <DialogDescription className="break-all" title={file}>
            {fileName(file)}
          </DialogDescription>
        </DialogHeader>
        {inspect.isPending && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {t("tools.loading")}
          </p>
        )}
        {inspect.isError && <p className="text-xs text-danger">{errorMessage(inspect.error)}</p>}
        {out?.status === "rejected" && (
          <p className="text-xs break-words text-danger">{failureText(out.reason, out.message)}</p>
        )}
        {info && <PatchPreview info={info} />}
        <DialogFooter>
          <Button size="sm" variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          {info && info.commits > 0 && (
            <Button
              size="sm"
              variant="outline"
              disabled={busy !== null}
              title={t("patch.asCommitsTip")}
              onClick={() => void run("commits")}
            >
              {busy === "commits" && <LoaderCircle className="animate-spin" />}
              {t("patch.asCommits", { count: info.commits })}
            </Button>
          )}
          <Button
            size="sm"
            disabled={busy !== null || !info}
            title={t("patch.asChangesTip")}
            onClick={() => void run("changes")}
          >
            {busy === "changes" && <LoaderCircle className="animate-spin" />}
            {t("patch.asChanges")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
