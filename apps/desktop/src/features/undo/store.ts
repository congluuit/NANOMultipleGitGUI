import { create } from "zustand";
import type { UndoState, UndoStep } from "@/ipc";

interface UndoStore {
  /** Last known state per repository (for the palette and the shortcuts). */
  byRepo: Record<number, UndoState>;
  /** Pending confirmation before undoing an operation whose commits are pushed. */
  confirm: { repoId: number; step: UndoStep } | null;
  setState: (repoId: number, state: UndoState) => void;
  setConfirm: (confirm: { repoId: number; step: UndoStep } | null) => void;
}

export const useUndoStore = create<UndoStore>()((set) => ({
  byRepo: {},
  confirm: null,
  setState: (repoId, state) => set((s) => ({ byRepo: { ...s.byRepo, [repoId]: state } })),
  setConfirm: (confirm) => set({ confirm }),
}));
