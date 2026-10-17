import { create } from "zustand";

interface StashUiState {
  /** Repository the "Stash changes" dialog is open for. */
  createFor: number | null;
  /** Repository the "Choose a stash" (Pop) dialog is open for. */
  pickerFor: number | null;
  openCreate: (repoId: number) => void;
  closeCreate: () => void;
  openPicker: (repoId: number) => void;
  closePicker: () => void;
}

export const useStashUi = create<StashUiState>()((set) => ({
  createFor: null,
  pickerFor: null,
  openCreate: (createFor) => set({ createFor }),
  closeCreate: () => set({ createFor: null }),
  openPicker: (pickerFor) => set({ pickerFor }),
  closePicker: () => set({ pickerFor: null }),
}));
