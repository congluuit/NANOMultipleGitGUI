import { create } from "zustand";

/** Rebase dialogs opened from menus, the palette or a branch drop. */
export type RebaseDialog =
  | {
      kind: "rebase";
      repoId: number;
      /** New base (branch, tag, commit id). */
      onto: string;
      ontoLabel: string;
      /** Branch to rebase; `null` = the current one. */
      branch: string | null;
    }
  | {
      kind: "interactive";
      repoId: number;
      base: string;
      baseLabel: string;
      /** Commit whose action starts as `drop` / `reword` ("Drop commit", "Edit message"). */
      preset?: { oid: string; action: "drop" | "reword" };
    };

interface RebaseDialogStore {
  dialog: RebaseDialog | null;
  open: (dialog: RebaseDialog | null) => void;
}

export const useRebaseDialog = create<RebaseDialogStore>()((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
}));

export const openRebaseDialog = (dialog: RebaseDialog) => useRebaseDialog.getState().open(dialog);
