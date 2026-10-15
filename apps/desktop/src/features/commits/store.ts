import { create } from "zustand";

/** Dialogs opened from the commit / branch context menus (the menu itself closes first). */
export type RefDialog =
  | { kind: "createBranch"; repoId: number; start: string; label: string }
  | { kind: "createTag"; repoId: number; oid: string; label: string; annotated?: boolean }
  | { kind: "resetHard"; repoId: number; oid: string; label: string; branch: string | null }
  | { kind: "renameBranch"; repoId: number; name: string }
  | { kind: "deleteBranch"; repoId: number; name: string; force: boolean }
  | { kind: "deleteTag"; repoId: number; name: string }
  | { kind: "merge"; repoId: number; rev: string; into: string | null }
  | {
      kind: "deleteRemoteBranch";
      repoId: number;
      remote: string;
      branch: string;
      /** Also delete this local branch (first). */
      local: string | null;
    }
  | { kind: "addWorktree"; repoId: number; branch: string };

interface RefDialogStore {
  dialog: RefDialog | null;
  open: (dialog: RefDialog | null) => void;
}

export const useRefDialog = create<RefDialogStore>()((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
}));
