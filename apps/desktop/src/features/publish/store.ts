import { create } from "zustand";

/** "new": create a repository on GitHub / GitLab; "url": add a remote by URL. */
export type PublishMode = "new" | "url";

export interface PublishPreset {
  repoId: number;
  mode: PublishMode;
}

interface PublishDialogState {
  preset: PublishPreset | null;
  open: (repoId: number, mode?: PublishMode) => void;
  close: () => void;
}

export const usePublishDialog = create<PublishDialogState>()((set) => ({
  preset: null,
  open: (repoId, mode = "new") => set({ preset: { repoId, mode } }),
  close: () => set({ preset: null }),
}));

/** Opens "Publish repository…" for an open repository. */
export const openPublishDialog = (repoId: number, mode: PublishMode = "new") =>
  usePublishDialog.getState().open(repoId, mode);
