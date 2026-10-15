/** Palette commands of the AI commit-message helper (6.4). */
import { Sparkles } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import type { AiSettingsView } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { normalizeConfig } from "./model";
import { aiKeys } from "./queries";
import { openAiSettings, useAiUi } from "./store";
import { applySuggestion, requestSuggestion } from "./suggest";

const aiEnabled = () =>
  normalizeConfig(queryClient.getQueryData<AiSettingsView>(aiKeys.settings)?.config).enabled;

export function aiCommands(c: PaletteContext): PaletteCommand[] {
  return [
    {
      id: "ai.settings",
      title: c.t("ai.cmd.settings"),
      group: c.t("cmd.group.app"),
      icon: Sparkles,
      keywords: ["ai", "ollama", "openai", "claude", "gemini", "commit message", "goi y"],
      run: openAiSettings,
    },
    {
      id: "ai.suggest",
      title: c.t("ai.cmd.suggest"),
      group: c.t("cmd.group.app"),
      icon: Sparkles,
      keywords: ["ai", "commit message", "goi y", "viet commit"],
      when: (x) =>
        x.repo !== null && aiEnabled() && useAiUi.getState().generating[x.repo.id] === undefined,
      run: async (x) => {
        const repoId = x.repo?.id;
        if (repoId === undefined) return;
        // No prompt from the palette: the old text comes back with "Hoàn tác".
        const run = async (unstaged: boolean): Promise<void> => {
          const s = await requestSuggestion(x.t, repoId, unstaged, (u) => void run(u));
          if (s) applySuggestion(repoId, { summary: s.summary, body: s.body });
        };
        await run(false);
      },
    },
  ];
}

registerCommands("ai", aiCommands);
