/** Palette command "Chạy lại hướng dẫn ban đầu" (3B.2). */
import { Compass } from "lucide-react";
import { registerCommands, type PaletteContext } from "@/features/command-palette/registry";
import { useSetupDialog } from "./store";

export function setupCommands(c: PaletteContext) {
  return [
    {
      id: "setup.rerun",
      title: c.t("setup.cmd.rerun"),
      group: c.t("cmd.group.app"),
      icon: Compass,
      keywords: ["onboarding", "setup", "first run", "huong dan", "kiem tra cong cu", "cai git"],
      run: () => useSetupDialog.getState().openSetup("welcome"),
    },
  ];
}

registerCommands("onboarding.setup", setupCommands);
