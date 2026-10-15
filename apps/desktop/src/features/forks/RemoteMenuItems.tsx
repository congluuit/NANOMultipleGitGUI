import { Copy, GitFork } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenuItem } from "@/components/ui/context-menu";
import { copyText } from "@/features/accounts/manager/openExternal";
import { openForkDialog } from "./store";

/** Context menu items of a remote row in the sidebar ("Fork repo này…", copy URL). */
export function RemoteMenuItems({
  repoId,
  remote,
  url,
}: {
  repoId: number;
  remote: string;
  url: string | null;
}) {
  const { t } = useTranslation();
  return (
    <>
      <ContextMenuItem onSelect={() => openForkDialog(repoId, remote)}>
        <GitFork />
        {t("forks.menu")}
      </ContextMenuItem>
      {url && (
        <ContextMenuItem onSelect={() => void copyText(url)}>
          <Copy />
          {t("forks.copyUrl")}
        </ContextMenuItem>
      )}
    </>
  );
}
