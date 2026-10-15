import { create } from "zustand";
import type { AvailableUpdate, UpdateError, UpdateProgress } from "@/ipc";

interface UpdaterUiState {
  /** The update shown in the dialog; `null` = closed. */
  update: AvailableUpdate | null;
  currentVersion: string;
  installing: boolean;
  progress: UpdateProgress | null;
  error: UpdateError | null;
  /** A check started from the menu / Settings is running. */
  checking: boolean;
  show: (update: AvailableUpdate, currentVersion: string) => void;
  close: () => void;
  setChecking: (checking: boolean) => void;
  setProgress: (progress: UpdateProgress | null) => void;
  startInstall: () => void;
  failInstall: (error: UpdateError) => void;
}

export const useUpdater = create<UpdaterUiState>()((set) => ({
  update: null,
  currentVersion: "",
  installing: false,
  progress: null,
  error: null,
  checking: false,
  show: (update, currentVersion) =>
    set({ update, currentVersion, installing: false, progress: null, error: null }),
  close: () => set({ update: null, installing: false, progress: null, error: null }),
  setChecking: (checking) => set({ checking }),
  setProgress: (progress) => set({ progress }),
  startInstall: () => set({ installing: true, progress: null, error: null }),
  failInstall: (error) => set({ installing: false, progress: null, error }),
}));
