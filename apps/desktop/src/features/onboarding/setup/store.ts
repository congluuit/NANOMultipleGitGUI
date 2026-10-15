import { create } from "zustand";
import type { SetupStep } from "@/ipc";

/**
 * The first-run setup dialog. Its step lives here (not in the component) because the dialog is
 * hidden while a dialog it opened (sign-in, import wizard, clone) is on screen.
 */
interface SetupDialog {
  open: boolean;
  step: SetupStep;
  openSetup: (step?: SetupStep) => void;
  setStep: (step: SetupStep) => void;
  close: () => void;
}

export const useSetupDialog = create<SetupDialog>()((set) => ({
  open: false,
  step: "welcome",
  openSetup: (step = "welcome") => set({ open: true, step }),
  setStep: (step) => set({ step }),
  close: () => set({ open: false }),
}));
