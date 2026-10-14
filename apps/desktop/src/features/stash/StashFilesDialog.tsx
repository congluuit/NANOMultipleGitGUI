import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { fileStatusLetter, splitPath } from "@/features/commit/format";
import { commands, errorMessage, type ChangedFile, type StashDetails } from "@/ipc";
import { cn } from "@/lib/utils";
import { stashName } from "./actions";

const STATUS_CLASS: Record<string, string> = {
  A: "text-success",
  M: "text-warning",
  D: "text-danger",
  R: "text-info",
  C: "text-info",
  T: "text-warning",
};

function FileRow({ file }: { file: ChangedFile }) {
  const { t } = useTranslation();
  const letter = fileStatusLetter(file.status);
  const { dir, name } = splitPath(file.path);
  return (
    <li
      className="flex items-center gap-2 px-2 py-0.5 text-xs"
      title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
    >
      <span className={cn("w-3 shrink-0 font-mono font-bold", STATUS_CLASS[letter])}>{letter}</span>
      <span className="min-w-0 flex-1 truncate">
        <span className="text-muted-foreground">{dir}</span>
        {name}
      </span>
      {file.additions === null ? (
        <span className="shrink-0 text-[11px] text-muted-foreground">{t("stash.binary")}</span>
      ) : (
        <span className="shrink-0 font-mono text-[11px] tabular-nums">
          <span className="text-success">+{file.additions}</span>{" "}
          <span className="text-danger">−{file.deletions}</span>
        </span>
      )}
    </li>
  );
}

/** Tracked changes, then untracked files, of one stash entry. */
export function StashFileList({ details }: { details: StashDetails }) {
  const { t } = useTranslation();
  if (details.files.length === 0 && details.untracked.length === 0) {
    return <p className="text-xs text-muted-foreground italic">{t("stash.noFiles")}</p>;
  }
  return (
    <div className="max-h-[50vh] space-y-3 overflow-y-auto">
      {details.files.length > 0 && (
        <section>
          <h3 className="mb-1 text-[11px] font-semibold text-muted-foreground uppercase">
            {t("stash.tracked", { count: details.files.length })}
          </h3>
          <ul className="rounded-md border bg-background py-1">
            {details.files.map((f) => (
              <FileRow key={f.path} file={f} />
            ))}
          </ul>
        </section>
      )}
      {details.untracked.length > 0 && (
        <section>
          <h3 className="mb-1 text-[11px] font-semibold text-muted-foreground uppercase">
            {t("stash.untracked", { count: details.untracked.length })}
          </h3>
          <ul className="rounded-md border bg-background py-1">
            {details.untracked.map((f) => (
              <FileRow key={f.path} file={f} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

interface Props {
  repoId: number;
  /** Stash entry to show; `null` = closed. */
  index: number | null;
  onClose: () => void;
}

export function StashFilesDialog({ repoId, index, onClose }: Props) {
  const { t } = useTranslation();
  const details = useQuery({
    queryKey: ["stashShow", repoId, index],
    queryFn: () => commands.stashShow(repoId, index!),
    enabled: isTauri() && index !== null,
    gcTime: 0,
  });
  const d = details.data;

  return (
    <Dialog open={index !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("stash.filesTitle", { name: stashName(index ?? 0) })}</DialogTitle>
          <DialogDescription className="break-words">
            {d
              ? `${d.message} · ${t("stash.summary", {
                  files: d.files.length + d.untracked.length,
                  additions: d.additions,
                  deletions: d.deletions,
                })}`
              : " "}
          </DialogDescription>
        </DialogHeader>
        {details.isPending && index !== null && isTauri() && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {t("stash.loading")}
          </p>
        )}
        {details.isError && (
          <p className="text-xs text-danger">
            {t("stash.error", { message: errorMessage(details.error) })}
          </p>
        )}
        {d && <StashFileList details={d} />}
      </DialogContent>
    </Dialog>
  );
}
