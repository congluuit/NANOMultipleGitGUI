import { FolderX } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { RepoTab } from "@/ipc";
import { closeRepository, forgetRepository, openRepository } from "./actions";

export function MissingRepo({ tab }: { tab: RepoTab }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 bg-background p-8 text-center">
      <FolderX className="size-10 text-warning" />
      <h2 className="text-sm font-semibold">{t("repos.missingTitle")}</h2>
      <p className="max-w-md text-xs break-all text-muted-foreground">
        {t("repos.missingBody", { path: tab.path })}
      </p>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" onClick={() => void openRepository(tab.path)}>
          {t("repos.retry")}
        </Button>
        <Button size="sm" variant="outline" onClick={() => void closeRepository(tab.id)}>
          {t("tabs.close")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => void forgetRepository(tab.id)}>
          {t("repos.remove")}
        </Button>
      </div>
    </div>
  );
}
