/** Palette commands of the less common tools. Registered when `ToolsHost` loads. */
import {
  Bug,
  ChartNoAxesColumn,
  FileDiff,
  FileInput,
  FolderTree,
  History,
  LayoutList,
  SearchCheck,
} from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { normalizeConfig } from "@/features/ai/model";
import { aiKeys } from "@/features/ai/queries";
import type { AiSettingsView } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { pickPatchToApply, saveChangesPatch } from "./actions";
import { openTool } from "./store";

const aiEnabled = () =>
  normalizeConfig(queryClient.getQueryData<AiSettingsView>(aiKeys.settings)?.config).enabled;

export function toolCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  const out: PaletteCommand[] = [
    {
      id: "repos.overview",
      title: t("overview.cmd"),
      group: t("cmd.group.repo"),
      icon: LayoutList,
      keywords: ["all repositories", "fetch all", "pull all", "overview", "tat ca repo"],
      when: (x) => x.tabs.length > 0,
      run: () => openTool({ kind: "overview" }),
    },
  ];
  if (!repo) return out;
  const id = repo.id;
  const group = t("cmd.group.history");
  out.push(
    {
      id: "history.reflog",
      title: t("reflog.cmd"),
      group,
      icon: History,
      keywords: ["reflog", "lost commit", "recover", "tim lai commit"],
      run: () => openTool({ kind: "reflog", repoId: id }),
    },
    {
      id: "history.bisect",
      title: t("bisect.cmd"),
      group,
      icon: Bug,
      keywords: ["bisect", "find bug", "regression", "tim loi"],
      when: (x) => !x.busy,
      run: () => openTool({ kind: "bisectStart", repoId: id, good: null, bad: null }),
    },
    {
      id: "history.stats",
      title: t("stats.cmd"),
      group,
      icon: ChartNoAxesColumn,
      keywords: ["statistics", "contributors", "activity", "thong ke"],
      run: () => openTool({ kind: "stats", repoId: id }),
    },
    {
      id: "patch.apply",
      title: t("patch.cmd.apply"),
      group: t("cmd.group.commit"),
      icon: FileInput,
      keywords: ["patch", "apply", "am", "diff file", "ap patch"],
      when: (x) => !x.busy,
      run: () => pickPatchToApply(id),
    },
    {
      id: "patch.saveChanges",
      title: t("patch.cmd.saveChanges"),
      group: t("cmd.group.commit"),
      icon: FileDiff,
      keywords: ["patch", "save", "export", "diff", "luu patch"],
      run: () => saveChangesPatch(id, "uncommitted"),
    },
    {
      id: "repo.sparse",
      title: t("sparse.cmd"),
      group: t("cmd.group.repo"),
      icon: FolderTree,
      keywords: ["sparse checkout", "partial", "folders", "thu muc"],
      when: (x) => !x.busy,
      run: () => openTool({ kind: "sparse", repoId: id }),
    },
    {
      id: "ai.review",
      title: t("aiReview.cmd"),
      group: t("cmd.group.commit"),
      icon: SearchCheck,
      keywords: ["ai", "review", "check changes", "kiem tra"],
      when: () => aiEnabled(),
      run: () => openTool({ kind: "aiReview", repoId: id }),
    },
  );
  return out;
}

registerCommands("tools", toolCommands);
