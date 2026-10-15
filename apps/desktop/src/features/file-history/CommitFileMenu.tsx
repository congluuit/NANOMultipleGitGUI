import { useState, type ReactElement } from "react";
import { Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type { ChangedFile } from "@/ipc";
import { FileViewMenuItems } from "./FileMenuItems";

/**
 * Right-click menu for the file list of "Commit details" (history, blame / view at this
 * commit). `children` is the list element; rows carry `data-path`.
 */
export function CommitFilesMenu({
  repoId,
  commit,
  files,
  children,
}: {
  repoId: number;
  commit: string;
  files: ChangedFile[];
  children: ReactElement;
}) {
  const { t } = useTranslation();
  const [target, setTarget] = useState<ChangedFile | null>(null);
  return (
    <ContextMenu>
      <ContextMenuTrigger
        asChild
        onContextMenu={(e) => {
          const el = (e.target as HTMLElement).closest<HTMLElement>("[data-path]");
          const file = el ? files.find((f) => f.path === el.dataset.path) : undefined;
          if (!file) {
            e.preventDefault();
            return;
          }
          setTarget(file);
        }}
      >
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent>
        {target && (
          <>
            <FileViewMenuItems
              repoId={repoId}
              path={target.path}
              commit={commit}
              deleted={target.status === "deleted"}
            />
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(target.path)}>
              <Copy />
              {t("wip.copyPath")}
            </ContextMenuItem>
          </>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
