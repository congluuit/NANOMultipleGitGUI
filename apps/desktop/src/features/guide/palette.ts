/** Phase 6.7: palette command "Mở hướng dẫn đầy đủ". */
import { BookOpen } from "lucide-react";
import { registerCommands, type PaletteContext } from "@/features/command-palette/registry";
import { openGuide } from "./store";

export function guideCommands(c: PaletteContext) {
  return [
    {
      id: "guide.open",
      title: c.t("guide.cmd"),
      group: c.t("cmd.group.app"),
      icon: BookOpen,
      keywords: ["guide", "help", "manual", "docs", "huong dan", "tro giup", "cach dung"],
      run: () => openGuide(),
    },
  ];
}

registerCommands("guide", guideCommands);
