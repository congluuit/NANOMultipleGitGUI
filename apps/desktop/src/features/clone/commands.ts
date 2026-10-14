/** Command palette contribution: "Clone repository…" (PLAN §4.7). */
import { Download } from "lucide-react";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { registerCommands, type PaletteContext } from "@/features/command-palette/registry";

export const openCloneDialog = (opts?: { accountId?: string | null; url?: string | null }) =>
  useAccountDialogs.getState().openClone(opts);

export function cloneCommands(c: PaletteContext) {
  return [
    {
      id: "clone.open",
      title: c.t("clone.open"),
      group: c.t("cmd.group.repo"),
      icon: Download,
      keywords: ["clone repository", "git clone", "nhan ban repo", "tai repo"],
      run: () => openCloneDialog(),
    },
  ];
}

registerCommands("clone", cloneCommands);
