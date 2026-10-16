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
  /**
   * The newest update found in this session that is not installed or skipped. Stays after
   * "Later" so the Settings button can show a dot and offer it again.
   */
  pending: { update: AvailableUpdate; currentVersion: string } | null;
  show: (update: AvailableUpdate, currentVersion: string) => void;
  close: () => void;
  setChecking: (checking: boolean) => void;
  setProgress: (progress: UpdateProgress | null) => void;
  startInstall: () => void;
  failInstall: (error: UpdateError) => void;
  /** Opens the dialog again for `pending`. */
  reopen: () => void;
  clearPending: () => void;
}

export const useUpdater = create<UpdaterUiState>()((set) => ({
  update: null,
  currentVersion: "",
  installing: false,
  progress: null,
  error: null,
  checking: false,
  pending: null,
  show: (update, currentVersion) =>
    set({
      update,
      currentVersion,
      installing: false,
      progress: null,
      error: null,
      pending: { update, currentVersion },
    }),
  close: () => set({ update: null, installing: false, progress: null, error: null }),
  setChecking: (checking) => set({ checking }),
  setProgress: (progress) => set({ progress }),
  startInstall: () => set({ installing: true, progress: null, error: null }),
  failInstall: (error) => set({ installing: false, progress: null, error }),
  reopen: () =>
    set((s) =>
      s.pending
        ? {
            update: s.pending.update,
            currentVersion: s.pending.currentVersion,
            installing: false,
            progress: null,
            error: null,
          }
        : {},
    ),
  clearPending: () => set({ pending: null }),
}));
