/** Palette commands of step 3.3: submodules, linked worktrees and the LFS manager. */
import { Eraser, HardDrive, Layers, Lock, Package } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import {
  notifyNothingToPrune,
  previewPrune,
  pruneWorktrees,
} from "@/features/linked-worktrees/actions";
import { useLfsManager } from "@/features/lfs/managerStore";
import { updateSubmodules } from "./actions";

export function advancedCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.repo");
  const lfs = (tab: "files" | "locks" | "maintenance") => () =>
    useLfsManager.getState().open(id, tab);
  return [
    {
      id: "submodule.update",
      title: t("submodule.cmd.update"),
      group,
      icon: Layers,
      keywords: ["submodule", "update", "init", "cap nhat"],
      when: (x) => !x.busy,
      run: () => void updateSubmodules(id, [], true),
    },
    {
      id: "worktrees.prune",
      title: t("worktrees.cmd.prune"),
      group,
      icon: Eraser,
      keywords: ["worktree", "prune", "don"],
      run: async () => {
        const lines = await previewPrune(id);
        if (lines && lines.length === 0) notifyNothingToPrune();
        else if (lines) await pruneWorktrees(id);
      },
    },
    {
      id: "lfs.manager",
      title: t("lfs.cmd.manager"),
      group,
      icon: Package,
      keywords: ["lfs", "large file", "pointer", "tai file"],
      run: lfs("files"),
    },
    {
      id: "lfs.locks",
      title: t("lfs.cmd.locks"),
      group,
      icon: Lock,
      keywords: ["lfs", "lock", "unlock", "khoa"],
      run: lfs("locks"),
    },
    {
      id: "lfs.maintenance",
      title: t("lfs.cmd.maintenance"),
      group,
      icon: HardDrive,
      keywords: ["lfs", "prune", "fsck", "disk", "giai phong"],
      run: lfs("maintenance"),
    },
  ];
}

registerCommands("core.advanced", advancedCommands);
