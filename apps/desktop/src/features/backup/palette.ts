/** Palette commands of steps 6.3 / 6.6. Registered when `BackupHost` loads. */
import { Download, HardDriveDownload, History, Upload } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { openBackupDialog } from "./store";

export function backupCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  const group = t("cmd.group.app");
  return [
    {
      id: "backup.create",
      title: t("backup.cmd.create"),
      group,
      icon: HardDriveDownload,
      keywords: ["backup", "export", "sao luu", "luu tru", "doi may"],
      run: () => openBackupDialog("fullCreate"),
    },
    {
      id: "backup.restore",
      title: t("backup.cmd.restore"),
      group,
      icon: History,
      keywords: ["restore", "backup", "khoi phuc", "sao luu", "may moi"],
      run: () => openBackupDialog("fullRestore"),
    },
    {
      id: "backup.exportAccounts",
      title: t("backup.cmd.exportAccounts"),
      group: t("cmd.group.accounts"),
      icon: Download,
      keywords: ["export", "accounts", "xuat", "tai khoan", "cau hinh"],
      run: () => openBackupDialog("accountsExport"),
    },
    {
      id: "backup.importAccounts",
      title: t("backup.cmd.importAccounts"),
      group: t("cmd.group.accounts"),
      icon: Upload,
      keywords: ["import", "accounts", "nhap", "tai khoan", "cau hinh"],
      run: () => openBackupDialog("accountsImport"),
    },
  ];
}

registerCommands("backup", backupCommands);
