import { Archive, ArchiveRestore, Settings, SquareTerminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Separator } from "@/components/ui/separator";
import { BranchDropHost } from "@/features/dnd/BranchDropMenu";
import { RebaseDialogsHost } from "@/features/rebase/RebaseDialogsHost";
import { RemoteButtons } from "@/features/remote/RemoteButtons";
import { useUi } from "@/stores/ui";
import { useActiveRepo } from "@/stores/workspace";
import { StashCreateDialogHost } from "@/features/stash/StashCreateDialog";
import { useStashToolbar } from "@/features/stash/useStashToolbar";
import { useTerminal } from "@/features/terminal/store";
import { useTerminalShortcut } from "@/features/terminal/useTerminalShortcut";
import { UndoButtons } from "@/features/undo/UndoButtons";
import { ToolButton } from "./ToolButton";

export function Toolbar() {
  const { t } = useTranslation();
  const openSettings = useUi((s) => s.setSettingsOpen);
  const stash = useStashToolbar();
  const repo = useActiveRepo();
  const repoId = repo && !repo.missing ? repo.id : null;
  const toggleTerminal = useTerminal((s) => s.toggle);
  useTerminalShortcut(repoId);

  return (
    <div className="flex h-12 shrink-0 items-center gap-0.5 border-b bg-surface px-2">
      {repo && !repo.missing && <RemoteButtons key={`remote-${repo.id}`} repoId={repo.id} />}
      <ToolButton
        icon={Archive}
        label={t("toolbar.stash")}
        onClick={stash.stash}
        disabled={!stash.canStash}
      />
      <ToolButton
        icon={ArchiveRestore}
        label={t("toolbar.pop")}
        onClick={() => void stash.pop()}
        disabled={!stash.canPop}
      />
      <StashCreateDialogHost />
      <Separator orientation="vertical" className="mx-1 !h-6" />
      <UndoButtons key={`undo-${repoId ?? "none"}`} repoId={repoId} />
      <RebaseDialogsHost />
      <BranchDropHost />
      <div className="flex-1" />
      <ToolButton
        icon={SquareTerminal}
        label={t("toolbar.terminal")}
        onClick={() => repoId !== null && toggleTerminal(repoId)}
        disabled={repoId === null}
      />
      <ToolButton
        icon={Settings}
        label={t("toolbar.settings")}
        onClick={() => openSettings(true)}
      />
    </div>
  );
}
