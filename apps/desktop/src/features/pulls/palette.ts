/** Palette commands of the pull / merge request feature (Phase 4.1). */
import { GitPullRequestCreate, GitPullRequestClosed } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { useHostingNav } from "@/stores/hosting";
import { openCreatePull } from "./store";

export function pullCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo, refs } = c;
  const commands: PaletteCommand[] = [];
  if (repo) {
    const branch = refs?.head.detached ? null : (refs?.head.branch ?? null);
    commands.push({
      id: "pulls.create",
      title: t("pulls.cmd.create"),
      group: t("cmd.group.repo"),
      icon: GitPullRequestCreate,
      keywords: ["pull request", "merge request", "pr", "mr", "tao", "create"],
      when: () => !!branch && !!refs && refs.remotes.length > 0,
      run: () => openCreatePull(repo.id, branch),
    });
  }
  if (useHostingNav.getState().pullRequest) {
    commands.push({
      id: "pulls.closeView",
      title: t("pulls.cmd.closeView"),
      group: t("cmd.group.repo"),
      icon: GitPullRequestClosed,
      keywords: ["pull request", "merge request", "close", "dong"],
      run: () => useHostingNav.getState().closePullRequest(),
    });
  }
  return commands;
}

registerCommands("pulls", pullCommands);
