import { create } from "zustand";

interface ShortcutsUi {
  open: boolean;
  setOpen: (open: boolean) => void;
}

/** The keyboard shortcut cheat sheet. */
export const useShortcutsUi = create<ShortcutsUi>()((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
}));

export function openShortcuts() {
  useShortcutsUi.getState().setOpen(true);
}
