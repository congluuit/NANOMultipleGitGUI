import { useTranslation } from "react-i18next";
import { useRepoTransfer } from "@/features/remote/transfers";
import { useRecentRepos, useRepoRefs } from "@/features/repos/queries";
import { useWorktreeStatus } from "@/features/worktree/queries";
import { useSettings } from "@/stores/settings";
import { useWorkspace } from "@/stores/workspace";
import type { PaletteContext } from "./registry";

/** Gathers the {@link PaletteContext} for the active repository (used while the palette is open). */
export function usePaletteContext(): PaletteContext {
  const { t } = useTranslation();
  const language = useSettings((s) => s.language);
  const theme = useSettings((s) => s.theme);
  const tabs = useWorkspace((s) => s.tabs);
  const activeId = useWorkspace((s) => s.activeId);
  const active = tabs.find((tab) => tab.id === activeId) ?? null;
  const repo = active && !active.missing ? active : null;
  const repoId = repo?.id ?? null;
  const refs = useRepoRefs(repoId, repo !== null).data ?? null;
  const status = useWorktreeStatus(repoId ?? -1, repo !== null).data ?? null;
  const busy = useRepoTransfer(repoId) !== null;
  const recentRepos = useRecentRepos().data ?? [];
  return {
    t,
    language,
    theme,
    isMac: navigator.userAgent.includes("Mac"),
    tabs,
    activeId,
    repo,
    refs: repo ? refs : null,
    status: repo ? status : null,
    busy,
    recentRepos,
  };
}
