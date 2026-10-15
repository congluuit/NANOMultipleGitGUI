import { create } from "zustand";
import { persist } from "zustand/middleware";
import { detectLanguage, type Language } from "@/i18n";
import type { ThemeMode } from "@/theme/theme";

/** Interface zoom levels offered in Settings; capped so the panes still fit the minimum window. */
export const UI_SCALES = [1, 1.1, 1.2, 1.25] as const;
export type UiScale = (typeof UI_SCALES)[number];

export interface SettingsValues {
  theme: ThemeMode;
  language: Language;
  /** CI results (badges, job lists, status bar). Off by default. */
  ciEnabled: boolean;
  /** Author avatars on the commit graph (loaded from GitHub / Gravatar). */
  authorAvatars: boolean;
  /** Text and controls size (webview zoom). Kept on this computer only. */
  uiScale: UiScale;
  /** Ask before a double-click / Enter in the sidebar switches branches. */
  confirmBranchSwitch: boolean;
}

interface SettingsState extends SettingsValues {
  setTheme: (theme: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setCiEnabled: (on: boolean) => void;
  setAuthorAvatars: (on: boolean) => void;
  setUiScale: (scale: UiScale) => void;
  setConfirmBranchSwitch: (on: boolean) => void;
}

export const defaultSettings = (): SettingsValues => ({
  theme: "system",
  language: detectLanguage(),
  ciEnabled: false,
  authorAvatars: true,
  uiScale: 1,
  confirmBranchSwitch: true,
});

export const useSettings = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaultSettings(),
      setTheme: (theme) => set({ theme }),
      setLanguage: (language) => set({ language }),
      setCiEnabled: (ciEnabled) => set({ ciEnabled }),
      setAuthorAvatars: (authorAvatars) => set({ authorAvatars }),
      setUiScale: (uiScale) => set({ uiScale }),
      setConfirmBranchSwitch: (confirmBranchSwitch) => set({ confirmBranchSwitch }),
    }),
    {
      name: "nanomgg.settings",
      version: 3,
      migrate: (persisted, version) => ({
        ...defaultSettings(),
        ...(persisted as Partial<SettingsValues>),
        ...(version < 2 ? { ciEnabled: false, authorAvatars: true } : {}),
      }),
    },
  ),
);
