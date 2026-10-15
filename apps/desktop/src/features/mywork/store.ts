import { create } from "zustand";

interface MyWorkUi {
  open: boolean;
  /** Only this account's rows (`null` = every account). */
  accountId: string | null;
  setOpen: (open: boolean) => void;
  setAccount: (accountId: string | null) => void;
}

export const useMyWorkUi = create<MyWorkUi>()((set) => ({
  open: false,
  accountId: null,
  setOpen: (open) => set({ open }),
  setAccount: (accountId) => set({ accountId }),
}));

export const openMyWork = () => useMyWorkUi.getState().setOpen(true);
