/** Palette commands of step 4.2: the "My work" panel. */
import { Inbox, RefreshCw } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { refreshMyWork } from "./queries";
import { openMyWork } from "./store";

export function myWorkCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  const group = t("issues.cmd.group");
  return [
    {
      id: "mywork.open",
      title: t("mywork.cmd.open"),
      group,
      icon: Inbox,
      keywords: ["my work", "review", "assigned", "inbox", "viec cua toi", "can review"],
      run: openMyWork,
    },
    {
      id: "mywork.refresh",
      title: t("mywork.cmd.refresh"),
      group,
      icon: RefreshCw,
      keywords: ["my work", "refresh", "lam moi"],
      run: () => refreshMyWork(),
    },
  ];
}

registerCommands("mywork", myWorkCommands);
