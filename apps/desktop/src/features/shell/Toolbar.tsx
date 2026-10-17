import { useRef } from "react";
import {
  Archive,
  ArchiveRestore,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  SquareTerminal,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Separator } from "@/components/ui/separator";
import { BranchDropHost } from "@/features/dnd/BranchDropMenu";
import { RebaseDialogsHost } from "@/features/rebase/RebaseDialogsHost";
import { RemoteButtons } from "@/features/remote/RemoteButtons";
import { shortcutLabel } from "./layoutShortcuts";
import { useLayout, usePanelShown } from "@/stores/layout";
import { useActiveRepo } from "@/stores/workspace";
import { StashCreateDialogHost } from "@/features/stash/StashCreateDialog";
import { StashPickerDialogHost } from "@/features/stash/StashPickerDialog";
import { StashPopMenu } from "@/features/stash/StashPopMenu";
import { useStashToolbar } from "@/features/stash/useStashToolbar";
import { useTerminal } from "@/features/terminal/store";
import { useTerminalShortcut } from "@/features/terminal/useTerminalShortcut";
import { UndoButtons } from "@/features/undo/UndoButtons";
import { MyWorkButton } from "@/features/mywork/MyWorkButton";
import { ToolButton } from "./ToolButton";
import { useToolbarOverflowLayout } from "./toolbarOverflow";
import { ToolbarOverflowMenu } from "./ToolbarOverflowMenu";

export function Toolbar() {
  const { t } = useTranslation();
  const stash = useStashToolbar();
  const repo = useActiveRepo();
  const repoId = repo && !repo.missing ? repo.id : null;
  const toggleTerminal = useTerminal((s) => s.toggle);
  useTerminalShortcut(repoId);
  const row = useRef<HTMLDivElement>(null);
  useToolbarOverflowLayout(row);
  const sidebarShown = usePanelShown("sidebar");
  const rightShown = usePanelShown("right");
  const togglePanel = useLayout((s) => s.togglePanel);
  const isMac = navigator.userAgent.includes("Mac");

  return (
    <div
      ref={row}
      role="toolbar"
      aria-label={t("layout.toolbar")}
      className="flex h-12 shrink-0 items-center gap-0.5 overflow-hidden border-b bg-surface px-2"
    >
      {repo && !repo.missing && <RemoteButtons key={`remote-${repo.id}`} repoId={repo.id} />}
      <ToolButton
        id="stash"
        icon={Archive}
        label={t("toolbar.stash")}
        onClick={stash.stash}
        disabled={!stash.canStash}
      />
      <ToolButton
        id="pop"
        icon={ArchiveRestore}
        label={t("toolbar.pop")}
        tooltip={stash.tooltip}
        tone={stash.tone}
        badge={
          stash.count > 0 && stash.tone ? { text: String(stash.count), tone: stash.tone } : null
        }
        onClick={() => void stash.pop()}
        disabled={!stash.canPop}
        menu={stash.count > 0 ? <StashPopMenu stash={stash} /> : undefined}
        menuLabel={t("stash.menuLabel")}
      />
      <StashCreateDialogHost />
      <StashPickerDialogHost />
      <Separator orientation="vertical" className="mx-1 !h-6" />
      <UndoButtons key={`undo-${repoId ?? "none"}`} repoId={repoId} />
      <RebaseDialogsHost />
      <BranchDropHost />
      <ToolbarOverflowMenu />
      <div className="flex-1" data-toolbar-flex="" />
      <MyWorkButton />
      <ToolButton
        id="sidebar"
        priority={0}
        icon={sidebarShown ? PanelLeftClose : PanelLeftOpen}
        label={t("layout.sidebarButton")}
        tooltip={`${t(sidebarShown ? "layout.collapseSidebar" : "layout.expandSidebar")} (${shortcutLabel("sidebar", isMac)})`}
        onClick={() => togglePanel("sidebar")}
      />
      <ToolButton
        id="rightPanel"
        priority={0}
        icon={rightShown ? PanelRightClose : PanelRightOpen}
        label={t("layout.rightButton")}
        tooltip={`${t(rightShown ? "layout.collapseRight" : "layout.expandRight")} (${shortcutLabel("right", isMac)})`}
        onClick={() => togglePanel("right")}
        disabled={repoId === null}
      />
      <ToolButton
        id="terminal"
        priority={5}
        icon={SquareTerminal}
        label={t("toolbar.terminal")}
        onClick={() => repoId !== null && toggleTerminal(repoId)}
        disabled={repoId === null}
      />
    </div>
  );
}
