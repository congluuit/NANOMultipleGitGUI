import { create } from "zustand";

/** Repos whose "unbound" banner was dismissed in this session (forgotten on restart). */
export const useBannerDismissed = create<{ ids: Record<number, true> }>()(() => ({ ids: {} }));

export const dismissBanner = (repoId: number) =>
  useBannerDismissed.setState((s) => ({ ids: { ...s.ids, [repoId]: true } }));
