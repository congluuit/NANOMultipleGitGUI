import { isTauri } from "@tauri-apps/api/core";
import { detectLanguage } from "@/i18n";
import { commands, errorMessage, type SettingsPatch } from "@/ipc";
import { useSettings, type SettingsValues } from "@/stores/settings";

export function diffSettings(prev: SettingsValues, next: SettingsValues): SettingsPatch | null {
  const patch: SettingsPatch = {};
  if (prev.theme !== next.theme) patch.theme = next.theme;
  if (prev.language !== next.language) patch.language = next.language;
  return Object.keys(patch).length ? patch : null;
}

/**
 * SQLite (via the backend) is the source of truth for settings; the persisted Zustand store
 * is only a local cache so the first paint already has the right theme.
 * Loads stored settings once, then writes every later change back. Returns an unsubscribe.
 */
export async function startSettingsSync(): Promise<() => void> {
  if (!isTauri()) return () => {};
  try {
    const stored = await commands.getSettings();
    useSettings.setState({
      theme: stored.theme,
      language: stored.language ?? detectLanguage(),
    });
  } catch (e) {
    console.error("Failed to load settings:", errorMessage(e));
  }
  return useSettings.subscribe((next, prev) => {
    const patch = diffSettings(prev, next);
    if (patch) {
      commands
        .updateSettings(patch)
        .catch((e: unknown) => console.error("Failed to save settings:", errorMessage(e)));
    }
  });
}
