import { create } from "zustand";

/** Phase 6.3 / 6.6 dialogs, opened from Settings, the account manager or the palette. */
export type BackupDialog = "accountsExport" | "accountsImport" | "fullCreate" | "fullRestore";

interface BackupUiState {
  dialog: BackupDialog | null;
  open: (dialog: BackupDialog) => void;
  close: () => void;
}

export const useBackupUi = create<BackupUiState>()((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
  close: () => set({ dialog: null }),
}));

export const openBackupDialog = (dialog: BackupDialog) => useBackupUi.getState().open(dialog);
