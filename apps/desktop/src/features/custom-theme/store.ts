// Phase 6.2: custom themes stored in SQLite (`theme.custom`, `theme.activeCustom`).
import { create } from "zustand";
import { isTauri } from "@tauri-apps/api/core";
import { commands, errorMessage, type CustomTheme, type CustomThemeInput } from "@/ipc";
import { useSettings } from "@/stores/settings";
import type { AppliedCustomTheme } from "@/theme/custom";
import { defaultCustomId, findTheme } from "./model";

interface CustomThemeState {
  /** The user's themes (presets are not stored). */
  themes: CustomTheme[];
  /** Theme used in "custom" mode: a preset id, a stored theme id or `null`. */
  activeId: string | null;
  /** The stored themes were read (never set when reading failed). */
  loaded: boolean;
  /** Colors shown while the theme editor is open (not saved); `null` = the real theme. */
  preview: AppliedCustomTheme | null;
  managerOpen: boolean;
  /** Theme selected when the manager opens. */
  managerFocus: string | null;
  load: () => Promise<void>;
  setPreview: (preview: AppliedCustomTheme | null) => void;
  openManager: (focus?: string | null) => void;
  closeManager: () => void;
  save: (input: CustomThemeInput) => Promise<CustomTheme>;
  remove: (id: string) => Promise<void>;
  removeAll: () => Promise<void>;
  /** Uses this theme and switches the theme mode to "custom". */
  activate: (id: string) => Promise<void>;
}

/** Back to "system" when the custom theme in use is gone. */
function fallBackIfMissing(themes: CustomTheme[], activeId: string | null) {
  const settings = useSettings.getState();
  if (settings.theme === "custom" && !findTheme(activeId, themes)) settings.setTheme("system");
}

export const useCustomThemes = create<CustomThemeState>()((set, get) => ({
  themes: [],
  activeId: null,
  loaded: false,
  preview: null,
  managerOpen: false,
  managerFocus: null,
  load: async () => {
    if (!isTauri()) {
      set({ loaded: true });
      return;
    }
    try {
      const state = await commands.customThemeList();
      set({ themes: state.themes, activeId: state.activeId, loaded: true });
    } catch (e) {
      // `loaded` stays false: the cached colors stay on screen and "custom" is not dropped.
      console.error("Failed to load custom themes:", errorMessage(e));
    }
  },
  setPreview: (preview) => set({ preview }),
  openManager: (focus = null) => set({ managerOpen: true, managerFocus: focus }),
  closeManager: () => set({ managerOpen: false, preview: null }),
  save: async (input) => {
    const saved = await commands.customThemeSave(input);
    set((s) => ({
      themes: s.themes.some((t) => t.id === saved.id)
        ? s.themes.map((t) => (t.id === saved.id ? saved : t))
        : [...s.themes, saved],
    }));
    return saved;
  },
  remove: async (id) => {
    const state = await commands.customThemeDelete(id);
    set({ themes: state.themes, activeId: state.activeId });
    fallBackIfMissing(state.themes, state.activeId);
  },
  removeAll: async () => {
    const state = await commands.customThemeDeleteAll();
    set({ themes: state.themes, activeId: state.activeId });
    fallBackIfMissing(state.themes, state.activeId);
  },
  activate: async (id) => {
    const previous = get().activeId;
    set({ activeId: id });
    useSettings.getState().setTheme("custom");
    if (!isTauri()) return;
    try {
      const state = await commands.customThemeSetActive(id);
      set({ themes: state.themes, activeId: state.activeId });
    } catch (e) {
      set({ activeId: previous });
      throw e;
    }
  },
}));

/** "Custom" in Settings / the command palette: keeps the chosen theme or picks a default one. */
export function enableCustomMode(): Promise<void> {
  const { themes, activeId, activate } = useCustomThemes.getState();
  return activate(defaultCustomId(themes, activeId)).catch((e: unknown) =>
    console.error("Failed to select the custom theme:", errorMessage(e)),
  );
}

export const openThemeManager = (focus?: string | null) =>
  useCustomThemes.getState().openManager(focus);
