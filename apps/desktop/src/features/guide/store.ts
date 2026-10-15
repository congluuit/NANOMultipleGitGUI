import { create } from "zustand";

interface GuideUi {
  open: boolean;
  /** Topic shown first (`null` = the first one). */
  topic: string | null;
  openGuide: (topic?: string | null) => void;
  close: () => void;
}

/** Phase 6.7: the Full Guide dialog. */
export const useGuide = create<GuideUi>()((set) => ({
  open: false,
  topic: null,
  openGuide: (topic = null) => set({ open: true, topic }),
  close: () => set({ open: false }),
}));

export function openGuide(topic?: string | null) {
  useGuide.getState().openGuide(topic);
}
