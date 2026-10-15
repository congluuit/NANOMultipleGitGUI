/** Command palette contribution: "Fork this repository…" (Phase 4.3). */
import { GitFork } from "lucide-react";
import { registerCommands, type PaletteContext } from "@/features/command-palette/registry";
import { openForkDialog } from "./store";

export function forkCommands(c: PaletteContext) {
  if (!c.repo) return [];
  const repoId = c.repo.id;
  return [
    {
      id: "forks.open",
      title: c.t("forks.paletteTitle"),
      group: c.t("cmd.group.repo"),
      icon: GitFork,
      keywords: ["fork repository", "fork this repo", "upstream", "tao fork", "nhanh ban sao"],
      run: () => openForkDialog(repoId),
    },
  ];
}

registerCommands("forks", forkCommands);
