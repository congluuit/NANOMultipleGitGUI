/** Palette command "Keyboard shortcuts". Registered when `ShortcutsHost` loads. */
import { Keyboard } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { openShortcuts } from "./store";

export function shortcutCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  return [
    {
      id: "app.shortcuts",
      title: t("shortcuts.title"),
      group: t("cmd.group.app"),
      icon: Keyboard,
      shortcut: "?",
      keywords: ["keyboard shortcuts", "hotkeys", "keys", "phim tat"],
      run: () => openShortcuts(),
    },
  ];
}

registerCommands("shortcuts", shortcutCommands);
