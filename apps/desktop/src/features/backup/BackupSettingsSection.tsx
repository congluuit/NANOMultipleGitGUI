import { HardDriveDownload, History } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useUi } from "@/stores/ui";
import { openBackupDialog, type BackupDialog } from "./store";

/** Phase 6.6: Settings → "Sao lưu & khôi phục". */
export function BackupSettingsSection() {
  const { t } = useTranslation();
  const open = (dialog: BackupDialog) => {
    useUi.getState().setSettingsOpen(false);
    openBackupDialog(dialog);
  };
  return (
    <div className="space-y-2" data-testid="backup-settings">
      <div className="text-xs font-semibold">{t("backup.section.title")}</div>
      <p className="text-[11px] text-muted-foreground">{t("backup.section.hint")}</p>
      <div className="grid grid-cols-2 gap-2">
        <Button variant="outline" size="sm" onClick={() => open("fullCreate")}>
          <HardDriveDownload />
          {t("backup.section.create")}
        </Button>
        <Button variant="outline" size="sm" onClick={() => open("fullRestore")}>
          <History />
          {t("backup.section.restore")}
        </Button>
      </div>
    </div>
  );
}
