import { Download, FolderOpen, FolderPlus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { openCloneDialog } from "@/features/clone/commands";
import { newRepositoryWithDialog, openRepositoryWithDialog } from "./actions";
import { RecentList } from "./RecentList";

export function WelcomeScreen() {
  const { t } = useTranslation();
  return (
    <div className="flex min-h-0 flex-1 justify-center overflow-y-auto bg-background p-8">
      <div className="w-full max-w-xl space-y-6 pt-8">
        <div className="flex items-center gap-4">
          <img src="/logo.png" alt="" width={56} height={56} />
          <div>
            <h1 className="text-xl font-bold">{t("repos.welcomeTitle")}</h1>
            <p className="text-sm text-muted-foreground">{t("repos.welcomeBody")}</p>
          </div>
        </div>
        <div className="flex gap-2">
          <Button onClick={() => void openRepositoryWithDialog(t("repos.pickOpen"))}>
            <FolderOpen />
            {t("repos.open")}
          </Button>
          <Button
            variant="outline"
            onClick={() => void newRepositoryWithDialog(t("repos.pickNew"))}
          >
            <FolderPlus />
            {t("repos.new")}
          </Button>
          <Button variant="outline" onClick={() => openCloneDialog()}>
            <Download />
            {t("clone.open")}
          </Button>
        </div>
        <section className="space-y-2">
          <h2 className="text-xs font-semibold text-muted-foreground uppercase">
            {t("repos.recent")}
          </h2>
          <RecentList />
        </section>
      </div>
    </div>
  );
}
