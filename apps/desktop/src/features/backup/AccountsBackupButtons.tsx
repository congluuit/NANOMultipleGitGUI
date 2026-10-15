import { Download, Upload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { openBackupDialog } from "./store";

/** Phase 6.3: "Export configuration…" / "Import configuration…" in Account manager → Accounts. */
export function AccountsBackupButtons({ canExport }: { canExport: boolean }) {
  const { t } = useTranslation();
  return (
    <div className="ml-auto flex shrink-0 gap-1.5">
      <Button
        size="sm"
        variant="outline"
        disabled={!canExport}
        onClick={() => openBackupDialog("accountsExport")}
      >
        <Download />
        {t("backup.accountsExport.button")}
      </Button>
      <Button size="sm" variant="outline" onClick={() => openBackupDialog("accountsImport")}>
        <Upload />
        {t("backup.accountsImport.button")}
      </Button>
    </div>
  );
}
