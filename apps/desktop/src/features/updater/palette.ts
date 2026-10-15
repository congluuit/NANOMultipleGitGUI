/** Palette command of step 3B.4: "Kiểm tra cập nhật". Registered when `UpdaterHost` loads. */
import { CloudDownload } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { checkForUpdates } from "./actions";

export function updaterCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  return [
    {
      id: "app.checkUpdates",
      title: t("updater.cmd.check"),
      group: t("cmd.group.app"),
      icon: CloudDownload,
      keywords: ["update", "check for updates", "upgrade", "version", "cap nhat", "phien ban"],
      run: () => checkForUpdates({ startup: false }),
    },
  ];
}

registerCommands("updater", updaterCommands);
