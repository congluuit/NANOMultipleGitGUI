import { create } from "zustand";

interface StashUiState {
  /** Repository the "Stash changes" dialog is open for. */
  createFor: number | null;
  openCreate: (repoId: number) => void;
  closeCreate: () => void;
}

export const useStashUi = create<StashUiState>()((set) => ({
  createFor: null,
  openCreate: (createFor) => set({ createFor }),
  closeCreate: () => set({ createFor: null }),
}));
