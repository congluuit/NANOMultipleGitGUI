import { create } from "zustand";
import { persist } from "zustand/middleware";
import { detectLanguage, type Language } from "@/i18n";
import type { ThemeMode } from "@/theme/theme";

export interface SettingsValues {
  theme: ThemeMode;
  language: Language;
  /** CI results (badges, job lists, status bar). Off by default. */
  ciEnabled: boolean;
  /** Author avatars on the commit graph (loaded from GitHub / Gravatar). */
  authorAvatars: boolean;
}

interface SettingsState extends SettingsValues {
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setCiEnabled: (on: boolean) => void;
  setAuthorAvatars: (on: boolean) => void;
}

export const defaultSettings = (): SettingsValues => ({
  theme: "system",
  language: detectLanguage(),
  ciEnabled: false,
  authorAvatars: true,
});

export const useSettings = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaultSettings(),
      setTheme: (theme) => set({ theme }),
      setLanguage: (language) => set({ language }),
      setCiEnabled: (ciEnabled) => set({ ciEnabled }),
      setAuthorAvatars: (authorAvatars) => set({ authorAvatars }),
    }),
    {
      name: "nanomgg.settings",
      version: 2,
      migrate: (persisted, version) => ({
        ...defaultSettings(),
        ...(persisted as Partial<SettingsValues>),
        ...(version < 2 ? { ciEnabled: false, authorAvatars: true } : {}),
      }),
    },
  ),
);
