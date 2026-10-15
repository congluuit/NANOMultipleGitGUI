import { create } from "zustand";
import type { AiPullProgress } from "@/ipc";
import type { DraftText } from "./model";

/** Draft before an AI suggestion replaced it, and the suggestion itself ("Hoàn tác" once). */
export interface AiUndo {
  previous: DraftText;
  applied: DraftText;
}

/** A model download running in the AI settings. */
export interface AiPull {
  requestId: string;
  model: string;
  progress: AiPullProgress | null;
}

interface AiUiState {
  /** Settings → "Gợi ý commit bằng AI" dialog. */
  open: boolean;
  /** Suggestion being generated, per repository (its request id). */
  generating: Record<number, string>;
  undo: Record<number, AiUndo>;
  pull: AiPull | null;
  setOpen: (open: boolean) => void;
  setGenerating: (repoId: number, requestId: string | null) => void;
  setUndo: (repoId: number, undo: AiUndo | null) => void;
  setPull: (pull: AiPull | null) => void;
}

export const useAiUi = create<AiUiState>()((set) => ({
  open: false,
  generating: {},
  undo: {},
  pull: null,
  setOpen: (open) => set({ open }),
  setGenerating: (repoId, requestId) =>
    set((s) => {
      const generating = { ...s.generating };
      if (requestId) generating[repoId] = requestId;
      else delete generating[repoId];
      return { generating };
    }),
  setUndo: (repoId, undo) =>
    set((s) => {
      const next = { ...s.undo };
      if (undo) next[repoId] = undo;
      else delete next[repoId];
      return { undo: next };
    }),
  setPull: (pull) => set({ pull }),
}));

export const openAiSettings = () => useAiUi.getState().setOpen(true);
