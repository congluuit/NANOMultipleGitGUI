import { create } from "zustand";

/** "Fork this repository…" dialog for an open repository, optionally preset to a remote. */
export interface ForkDialogPreset {
  repoId: number;
  remote: string | null;
}

interface ForkDialogState {
  preset: ForkDialogPreset | null;
  open: (repoId: number, remote?: string | null) => void;
  close: () => void;
}

export const useForkDialog = create<ForkDialogState>()((set) => ({
  preset: null,
  open: (repoId, remote = null) => set({ preset: { repoId, remote } }),
  close: () => set({ preset: null }),
}));

export const openForkDialog = (repoId: number, remote?: string | null) =>
  useForkDialog.getState().open(repoId, remote);
