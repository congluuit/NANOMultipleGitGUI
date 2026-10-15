import { Eye, GitMerge, History, UserRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenuItem } from "@/components/ui/context-menu";
import { openFileView } from "@/stores/fileView";

/**
 * File-view entries of a file context menu: history, blame and (inside a commit) the file as
 * of that commit. `commit` = the commit the menu belongs to; without it blame shows the working
 * tree.
 */
export function FileViewMenuItems({
  repoId,
  path,
  commit,
  deleted,
  conflicted,
}: {
  repoId: number;
  path: string | null;
  commit?: string;
  /** The file does not exist at `commit` (deleted there): no blame / view. */
  deleted?: boolean;
  conflicted?: boolean;
}) {
  const { t } = useTranslation();
  const disabled = path === null;
  return (
    <>
      {conflicted && (
        <ContextMenuItem
          disabled={disabled}
          onSelect={() => path && openFileView(repoId, { kind: "conflict", path })}
        >
          <GitMerge />
          {t("conflict.open")}
        </ContextMenuItem>
      )}
      <ContextMenuItem
        disabled={disabled}
        onSelect={() => path && openFileView(repoId, { kind: "history", path })}
      >
        <History />
        {t("fileHistory.menuHistory")}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={disabled || deleted}
        onSelect={() => path && openFileView(repoId, { kind: "blame", path, rev: commit ?? null })}
      >
        <UserRound />
        {commit ? t("fileHistory.blameHere") : t("fileHistory.menuBlame")}
      </ContextMenuItem>
      {commit && (
        <ContextMenuItem
          disabled={disabled || deleted}
          onSelect={() => path && openFileView(repoId, { kind: "file", path, commit })}
        >
          <Eye />
          {t("fileHistory.viewFile")}
        </ContextMenuItem>
      )}
    </>
  );
}
