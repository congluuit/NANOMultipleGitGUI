import { HardDrive, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatBytes } from "@/lib/format";
import { useUi } from "@/stores/ui";
import { useStorageStatus } from "./queries";
import { openStorageSettings } from "./store";

/** Settings → "Lưu trữ": free space of the data drive + entry to the Storage dialog. */
export function StorageSection() {
  const { t } = useTranslation();
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const status = useStorageStatus().data;

  return (
    <div className="space-y-2" data-testid="storage-settings">
      <div className="text-xs font-semibold">{t("storage.title")}</div>
      <div className="text-xs text-muted-foreground">{t("storage.sectionHint")}</div>
      {status?.lowSpace && status.dataFreeBytes !== null && (
        <p className="flex items-start gap-1.5 text-xs text-destructive" role="alert">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
          {t("storage.lowSpace", { free: formatBytes(status.dataFreeBytes) })}
        </p>
      )}
      {status?.pendingData && (
        <p className="text-xs break-all text-primary">
          {t("storage.pendingRow", { path: status.pendingData })}
        </p>
      )}
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() => {
          setSettingsOpen(false);
          openStorageSettings();
        }}
      >
        <HardDrive />
        {t("storage.manage")}
      </Button>
    </div>
  );
}
