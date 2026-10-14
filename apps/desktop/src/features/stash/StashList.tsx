import { useState } from "react";
import { ArchiveRestore, FileText, PackageOpen, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type { Stash } from "@/ipc";
import { applyStash, dropStash, stashName } from "./actions";
import { StashFilesDialog } from "./StashFilesDialog";

interface Props {
  repoId: number;
  stashes: Stash[];
}

/** Rows of the sidebar "Stashes" section, each with an Apply / Pop / Drop / Show files menu. */
export function StashList({ repoId, stashes }: Props) {
  const { t } = useTranslation();
  const [showing, setShowing] = useState<number | null>(null);
  const [dropping, setDropping] = useState<Stash | null>(null);

  const confirmDrop = () => {
    const target = dropping;
    setDropping(null);
    if (target) void dropStash(repoId, target.index);
  };

  return (
    <>
      {stashes.map((s) => (
        <ContextMenu key={s.index}>
          <ContextMenuTrigger asChild>
            <li
              title={`${stashName(s.index)}: ${s.message}`}
              onDoubleClick={() => setShowing(s.index)}
              className="flex h-6 w-full cursor-default items-center gap-2 truncate pr-2 pl-8 text-left text-xs hover:bg-surface-2 data-[state=open]:bg-surface-2"
            >
              <span className="truncate">{s.message}</span>
            </li>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuLabel>{stashName(s.index)}</ContextMenuLabel>
            <ContextMenuItem onSelect={() => void applyStash(repoId, s.index, false)}>
              <PackageOpen />
              {t("stash.apply")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => void applyStash(repoId, s.index, true)}>
              <ArchiveRestore />
              {t("stash.pop")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => setShowing(s.index)}>
              <FileText />
              {t("stash.showFiles")}
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem variant="destructive" onSelect={() => setDropping(s)}>
              <Trash2 />
              {t("stash.drop")}
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      ))}
      <StashFilesDialog repoId={repoId} index={showing} onClose={() => setShowing(null)} />
      <ConfirmDialog
        open={dropping !== null}
        title={t("stash.dropTitle", { name: stashName(dropping?.index ?? 0) })}
        description={t("stash.dropBody", { message: dropping?.message ?? "" })}
        confirmLabel={t("stash.dropConfirm")}
        destructive
        onConfirm={confirmDrop}
        onCancel={() => setDropping(null)}
      />
    </>
  );
}
