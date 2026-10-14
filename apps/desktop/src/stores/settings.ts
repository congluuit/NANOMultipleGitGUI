import { create } from "zustand";
import { persist } from "zustand/middleware";
import { detectLanguage, type Language } from "@/i18n";
import type { ThemeMode } from "@/theme/theme";

export interface SettingsValues {
  theme: ThemeMode;
  language: Language;
}

interface SettingsState extends SettingsValues {
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (language: Language) => void;
}

export const defaultSettings = (): SettingsValues => ({
  theme: "system",
  language: detectLanguage(),
});

export const useSettings = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaultSettings(),
      setTheme: (theme) => set({ theme }),
      setLanguage: (language) => set({ language }),
    }),
    { name: "nanomgg.settings", version: 1 },
  ),
);
