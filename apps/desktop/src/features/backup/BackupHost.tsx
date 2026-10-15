import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { commands } from "@/ipc";
import { useUi } from "@/stores/ui";
import { AccountsExportDialog } from "./AccountsExportDialog";
import { AccountsImportDialog } from "./AccountsImportDialog";
import { FullBackupDialog } from "./FullBackupDialog";
import { FullRestoreDialog } from "./FullRestoreDialog";
import "./palette";

/** Restore outcome of this start is read once per app run (also across StrictMode remounts). */
let noticeChecked = false;

/**
 * Mounted once in `App`: the four backup dialogs, their palette commands, and the notice
 * telling how a restore staged in the previous session went.
 */
export function BackupHost() {
  const { t, i18n } = useTranslation();

  useEffect(() => {
    if (noticeChecked || !isTauri()) return;
    noticeChecked = true;
    void commands
      .backupRestoreNotice()
      .then((notice) => {
        if (!notice) return;
        const ui = useUi.getState();
        if (notice.ok) {
          const date =
            notice.createdAt !== null
              ? new Date(notice.createdAt).toLocaleString(i18n.language)
              : "?";
          ui.showNotice({
            kind: "info",
            message: t("backup.full.restoredNotice", { date }),
            hint: notice.previousCopy
              ? t("backup.full.restoredHint", { path: notice.previousCopy })
              : undefined,
          });
        } else {
          ui.showNotice({
            kind: "error",
            message: t("backup.full.failedNotice"),
            hint: t("backup.full.failedHint"),
            detail: notice.error ?? undefined,
          });
        }
      })
      .catch(() => {});
  }, [t, i18n.language]);

  return (
    <>
      <AccountsExportDialog />
      <AccountsImportDialog />
      <FullBackupDialog />
      <FullRestoreDialog />
    </>
  );
}
