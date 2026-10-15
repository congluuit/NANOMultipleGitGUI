import { GitPullRequestCreate } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenuItem } from "@/components/ui/context-menu";
import { openCreatePull } from "./store";

/** "Create PR/MR from this branch…" for a local branch menu. */
export function PullBranchMenuItem({ repoId, branch }: { repoId: number; branch: string }) {
  const { t } = useTranslation();
  return (
    <ContextMenuItem onSelect={() => openCreatePull(repoId, branch)}>
      <GitPullRequestCreate />
      {t("pulls.menu.create")}
    </ContextMenuItem>
  );
}
