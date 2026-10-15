import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { type AiConfig, type AiSettingsView, commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { type FullAiConfig, normalizeConfig } from "./model";

export const aiKeys = {
  settings: ["ai", "settings"] as const,
  ollama: ["ai", "ollama"] as const,
};

export function useAiSettings() {
  return useQuery({
    queryKey: aiKeys.settings,
    queryFn: () => commands.aiSettings(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}

/** The config with defaults filled in (`null` until loaded). */
export function useAiConfig(): FullAiConfig | null {
  const data = useAiSettings().data;
  return data ? normalizeConfig(data.config) : null;
}

/** Stores the config; the cache is updated first so switches react at once. */
export async function saveAiConfig(config: AiConfig): Promise<AiSettingsView | undefined> {
  queryClient.setQueryData<AiSettingsView>(aiKeys.settings, (old) =>
    old ? { ...old, config } : old,
  );
  if (!isTauri()) return undefined;
  try {
    const view = await commands.aiSettingsSave(config);
    queryClient.setQueryData(aiKeys.settings, view);
    return view;
  } catch (e) {
    await queryClient.invalidateQueries({ queryKey: aiKeys.settings });
    throw e;
  }
}

export function setAiSettings(view: AiSettingsView) {
  queryClient.setQueryData(aiKeys.settings, view);
}

export function useOllamaStatus(enabled: boolean) {
  return useQuery({
    queryKey: aiKeys.ollama,
    queryFn: () => commands.aiOllamaStatus(),
    enabled: enabled && isTauri(),
    staleTime: 0,
    retry: false,
  });
}

export const refreshOllama = () => queryClient.invalidateQueries({ queryKey: aiKeys.ollama });
