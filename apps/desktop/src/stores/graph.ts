import { create } from "zustand";

export interface GraphSelection {
  hash: string;
  index: number;
}

interface JumpRequest {
  repoId: number;
  oid: string;
  nonce: number;
}

interface GraphStore {
  /** Selected commit per repository. Kept by hash so it survives history reloads. */
  selection: Record<number, GraphSelection | null>;
  jump: JumpRequest | null;
  select: (repoId: number, selection: GraphSelection | null) => void;
  /** Asks the graph of `repoId` to scroll to and select a commit (e.g. a branch tip). */
  requestJump: (repoId: number, oid: string) => void;
}

let nonce = 0;

export const useGraphStore = create<GraphStore>()((set) => ({
  selection: {},
  jump: null,
  select: (repoId, selection) =>
    set((s) => ({ selection: { ...s.selection, [repoId]: selection } })),
  requestJump: (repoId, oid) => set({ jump: { repoId, oid, nonce: ++nonce } }),
}));

export const useSelection = (repoId: number | null | undefined) =>
  useGraphStore((s) => (repoId == null ? null : (s.selection[repoId] ?? null)));
