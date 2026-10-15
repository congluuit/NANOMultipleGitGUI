import { create } from "zustand";
import type { BranchSwitchCheck, SwitchTarget } from "@/ipc";

/** A switch waiting for the user's answer (uncommitted changes / detached HEAD warning). */
export interface PendingSwitch {
  kind: "dirty" | "detached";
  repoId: number;
  target: SwitchTarget;
  check: BranchSwitchCheck;
}

interface BranchSwitchUi {
  pending: PendingSwitch | null;
  ask: (pending: PendingSwitch) => void;
  close: () => void;
}

export const useBranchSwitchUi = create<BranchSwitchUi>()((set) => ({
  pending: null,
  ask: (pending) => set({ pending }),
  close: () => set({ pending: null }),
}));
