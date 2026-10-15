import { create } from "zustand";

export type LfsTab = "patterns" | "files" | "locks" | "maintenance";

interface LfsManagerState {
  repoId: number | null;
  tab: LfsTab;
  open: (repoId: number, tab?: LfsTab) => void;
  setTab: (tab: LfsTab) => void;
  close: () => void;
}

/** Which repository's LFS manager dialog is open, on which tab. */
export const useLfsManager = create<LfsManagerState>()((set) => ({
  repoId: null,
  tab: "files",
  open: (repoId, tab = "files") => set({ repoId, tab }),
  setTab: (tab) => set({ tab }),
  close: () => set({ repoId: null }),
}));
