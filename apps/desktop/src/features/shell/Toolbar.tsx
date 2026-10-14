import type { LucideIcon } from "lucide-react";
import { Archive, ArchiveRestore, Redo2, Settings, SquareTerminal, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { RemoteButtons } from "@/features/remote/RemoteButtons";
import { useUi } from "@/stores/ui";
import { useActiveRepo } from "@/stores/workspace";
import { StashCreateDialogHost } from "@/features/stash/StashCreateDialog";
import { useStashToolbar } from "@/features/stash/useStashToolbar";
import { useTerminal } from "@/features/terminal/store";
import { useTerminalShortcut } from "@/features/terminal/useTerminalShortcut";

function ToolButton({
  icon: Icon,
  label,
  onClick,
  disabled,
}: {
  icon: LucideIcon;
  label: string;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          disabled={disabled}
          className="flex h-10 min-w-12 flex-col items-center justify-center gap-0.5 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-surface-2 hover:text-foreground disabled:opacity-40"
        >
          <Icon className="size-4" />
          <span>{label}</span>
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function Toolbar() {
  const { t } = useTranslation();
  const openSettings = useUi((s) => s.setSettingsOpen);
  const stash = useStashToolbar();
  const repo = useActiveRepo();
  const terminalRepo = repo && !repo.missing ? repo.id : null;
  const toggleTerminal = useTerminal((s) => s.toggle);
  useTerminalShortcut(terminalRepo);

  return (
    <div className="flex h-12 shrink-0 items-center gap-0.5 border-b bg-surface px-2">
      {repo && !repo.missing && <RemoteButtons key={repo.id} repoId={repo.id} />}
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
      <ToolButton icon={Undo2} label={t("toolbar.undo")} />
      <ToolButton icon={Redo2} label={t("toolbar.redo")} />
      <div className="flex-1" />
      <ToolButton
        icon={SquareTerminal}
        label={t("toolbar.terminal")}
        onClick={() => terminalRepo !== null && toggleTerminal(terminalRepo)}
        disabled={terminalRepo === null}
      />
      <ToolButton
        icon={Settings}
        label={t("toolbar.settings")}
        onClick={() => openSettings(true)}
      />
    </div>
  );
}
