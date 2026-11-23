/** Palette command "Search in all open repositories". */
import { Search } from "lucide-react";
import {
  modKey,
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { showSearch } from "./store";

export function searchCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  return [
    {
      id: "search.allRepos",
      title: t("search.all.cmd"),
      group: t("cmd.group.repo"),
      icon: Search,
      shortcut: `${modKey(c)}+${c.isMac ? "⌥" : "Alt"}+Shift+F`,
      keywords: ["search all repositories", "grep", "find in files", "tim trong moi repo"],
      // The panel lives in the sidebar of an open repository.
      when: () => c.repo !== null,
      run: () => showSearch("all"),
    },
  ];
}

registerCommands("files.search", searchCommands);
