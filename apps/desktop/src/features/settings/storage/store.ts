import { create } from "zustand";
import type { StorageMoveProgress } from "@/ipc";

interface StorageUiState {
  /** Settings → Storage dialog. */
  open: boolean;
  /** Last progress of the running log/cache move (`null` when idle). */
  progress: StorageMoveProgress | null;
  /** Startup warnings and the low-space notice were already shown this session. */
  noticesShown: boolean;
  setOpen: (open: boolean) => void;
  setProgress: (progress: StorageMoveProgress | null) => void;
  markNoticesShown: () => void;
}

export const useStorageUi = create<StorageUiState>()((set) => ({
  open: false,
  progress: null,
  noticesShown: false,
  setOpen: (open) => set({ open }),
  setProgress: (progress) => set({ progress }),
  markNoticesShown: () => set({ noticesShown: true }),
}));

export const openStorageSettings = () => useStorageUi.getState().setOpen(true);
