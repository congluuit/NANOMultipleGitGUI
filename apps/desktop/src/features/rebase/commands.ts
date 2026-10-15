/** Palette commands: rebase the current branch, interactive rebase from the selected commit. */
import { CloudDownload, GitBranch, GitPullRequestArrow, ListOrdered } from "lucide-react";
import { registerCommands, type PaletteCommand } from "@/features/command-palette/registry";
import { useGraphStore } from "@/stores/graph";
import { openRebaseDialog } from "./store";

registerCommands("rebase", (c): PaletteCommand[] => {
  const { t, refs, repo } = c;
  if (!repo) return [];
  const repoId = repo.id;
  const group = t("cmd.group.branch");
  const head = refs?.head;
  const current = head && !head.detached ? head.branch : null;
  const selected = useGraphStore.getState().selection[repoId] ?? null;
  const canRebase = current !== null && !head?.unborn;
  const local = (refs?.local ?? []).filter((b) => !b.isHead).map((b) => b.name);
  const remote = (refs?.remotes ?? []).flatMap((r) =>
    r.branches.filter((b) => b.name !== "HEAD").map((b) => `${r.name}/${b.name}`),
  );
  const short = (oid: string) => oid.slice(0, 7);
  return [
    {
      id: "rebase.onto",
      title: t("rebase.palette.rebaseOnto"),
      group,
      icon: GitPullRequestArrow,
      keywords: ["rebase", "rebase onto"],
      when: () => canRebase && local.length + remote.length > 0,
      children: () =>
        [...local.map((n) => [n, false] as const), ...remote.map((n) => [n, true] as const)].map(
          ([name, isRemote]) => ({
            id: `${isRemote ? "remote" : "local"}:${name}`,
            title: name,
            group: "",
            icon: isRemote ? CloudDownload : GitBranch,
            run: () =>
              openRebaseDialog({
                kind: "rebase",
                repoId,
                onto: name,
                ontoLabel: name,
                branch: null,
              }),
          }),
        ),
    },
    {
      id: "rebase.ontoSelected",
      title: t("rebase.palette.rebaseOntoSelected"),
      group,
      icon: GitPullRequestArrow,
      keywords: ["rebase", "selected commit"],
      when: () => canRebase && selected !== null && selected.hash !== head?.oid,
      run: () =>
        selected &&
        openRebaseDialog({
          kind: "rebase",
          repoId,
          onto: selected.hash,
          ontoLabel: short(selected.hash),
          branch: null,
        }),
    },
    {
      id: "rebase.interactive",
      title: t("rebase.palette.interactiveSelected"),
      group,
      icon: ListOrdered,
      keywords: ["interactive rebase", "rebase -i", "squash", "reword", "fixup", "reorder"],
      when: () => canRebase && selected !== null && selected.hash !== head?.oid,
      run: () =>
        selected &&
        openRebaseDialog({
          kind: "interactive",
          repoId,
          base: selected.hash,
          baseLabel: short(selected.hash),
        }),
    },
  ];
});
