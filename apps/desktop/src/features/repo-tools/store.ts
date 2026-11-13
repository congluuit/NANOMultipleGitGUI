import { create } from "zustand";

/** Dialogs of the less common git tools (one at a time). */
export type ToolDialog =
  | { kind: "overview" }
  | { kind: "reflog"; repoId: number }
  | { kind: "stats"; repoId: number }
  | { kind: "sparse"; repoId: number }
  | { kind: "patchApply"; repoId: number; file: string }
  | { kind: "bisectStart"; repoId: number; good: string | null; bad: string | null }
  | { kind: "aiReview"; repoId: number };

interface ToolsState {
  dialog: ToolDialog | null;
  open: (dialog: ToolDialog | null) => void;
}

export const useTools = create<ToolsState>()((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
}));

export const openTool = (dialog: ToolDialog) => useTools.getState().open(dialog);
export const closeTool = () => useTools.getState().open(null);
