import { create } from "zustand";
import type { BranchSwitchCheck, SwitchTarget } from "@/ipc";

/** A switch waiting for the user's answer (uncommitted changes / detached HEAD warning). */
export interface PendingSwitch {
  kind: "dirty" | "detached";
  repoId: number;
  target: SwitchTarget;
  check: BranchSwitchCheck;
}

/** A sidebar double-click / Enter waiting for "Switch to …?" (a misclick must not switch). */
export interface ConfirmSwitch {
  repoId: number;
  target: SwitchTarget;
}

interface BranchSwitchUi {
  pending: PendingSwitch | null;
  confirm: ConfirmSwitch | null;
  ask: (pending: PendingSwitch) => void;
  close: () => void;
  askConfirm: (confirm: ConfirmSwitch) => void;
  closeConfirm: () => void;
}

export const useBranchSwitchUi = create<BranchSwitchUi>()((set) => ({
  pending: null,
  confirm: null,
  ask: (pending) => set({ pending }),
  close: () => set({ pending: null }),
  askConfirm: (confirm) => set({ confirm }),
  closeConfirm: () => set({ confirm: null }),
}));
