import { GitMerge, History, UserRound } from "lucide-react";
import { registerCommands, type PaletteCommand } from "@/features/command-palette/registry";
import { openFileView } from "@/stores/fileView";

/** Repository-relative path typed by the user, normalised; `null` when unusable. */
export function normalizeRepoPath(value: string): string | null {
  const p = value.trim().replace(/\\/g, "/").replace(/^\.\//, "").replace(/^\/+/, "");
  if (!p || p.startsWith("-") || /^[a-zA-Z]:/.test(p) || p.split("/").includes("..")) return null;
  return p;
}

registerCommands("fileViews", (c): PaletteCommand[] => {
  const { t, repo, status } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.history");
  const conflicted = status?.conflicted ?? [];
  const pathInput = (run: (path: string) => void) => ({
    placeholder: t("fileHistory.cmdPlaceholder"),
    validate: (v: string) => (normalizeRepoPath(v) ? null : t("fileHistory.cmdInvalid")),
    run: (v: string) => {
      const p = normalizeRepoPath(v);
      if (p) run(p);
    },
  });
  return [
    {
      id: "conflict.resolve",
      title: t("conflict.cmdResolve"),
      group,
      icon: GitMerge,
      keywords: ["conflict", "resolve", "merge", "xung dot"],
      when: () => conflicted.length > 0,
      children: () =>
        conflicted.map((e) => ({
          id: e.path,
          title: e.path,
          group: "",
          hint: e.conflict ?? undefined,
          icon: GitMerge,
          run: () => openFileView(id, { kind: "conflict", path: e.path }),
        })),
    },
    {
      id: "file.history",
      title: t("fileHistory.cmdHistory"),
      group,
      icon: History,
      keywords: ["file history", "log follow", "lich su file"],
      input: pathInput((path) => openFileView(id, { kind: "history", path })),
    },
    {
      id: "file.blame",
      title: t("fileHistory.cmdBlame"),
      group,
      icon: UserRound,
      keywords: ["blame", "annotate", "who changed", "ai sua"],
      input: pathInput((path) => openFileView(id, { kind: "blame", path, rev: null })),
    },
  ];
});
