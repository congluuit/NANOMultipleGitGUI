import { create } from "zustand";
import { persist } from "zustand/middleware";

interface ChecklistState {
  /** "Hide" was clicked: the first-run checklist does not come back. */
  dismissed: boolean;
  /** The guide was opened from the checklist (its step counts as done). */
  guideSeen: boolean;
  dismiss: () => void;
  markGuideSeen: () => void;
}

/** First-run checklist on the Welcome screen (kept on this computer only). */
export const useChecklist = create<ChecklistState>()(
  persist(
    (set) => ({
      dismissed: false,
      guideSeen: false,
      dismiss: () => set({ dismissed: true }),
      markGuideSeen: () => set({ guideSeen: true }),
    }),
    { name: "nanomgg.firstRunChecklist", version: 1 },
  ),
);

export interface ChecklistStep {
  id: "account" | "repo" | "guide";
  done: boolean;
}

export function checklistSteps(input: {
  accounts: number;
  repos: number;
  guideSeen: boolean;
}): ChecklistStep[] {
  return [
    { id: "account", done: input.accounts > 0 },
    { id: "repo", done: input.repos > 0 },
    { id: "guide", done: input.guideSeen },
  ];
}
