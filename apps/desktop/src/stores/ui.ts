import { create } from "zustand";
import type { FailureKind } from "@/ipc";
import type { ResolvedTheme } from "@/theme/theme";

export type View = "workspace" | "palette";

export interface NoticeAction {
  label: string;
  run: () => void;
}

export interface Notice {
  /** Every notice hides itself after a few seconds (with a countdown, paused on hover). */
  kind: "error" | "info" | "success";
  message: string;
  /** What to do about it, in plain words (for users new to git). */
  hint?: string;
  /** Raw output (git's stderr), shown collapsed. */
  detail?: string;
  actions?: NoticeAction[];
  /** Why git refused the operation, when it was recognised (`failureNotice`). */
  reason?: FailureKind;
  /** Repository the failed operation ran in (recovery actions act on it). */
  repoId?: number;
  /** Runs the failed operation again (offered after a recovery fix). */
  retry?: () => void;
  /** Overrides how long the notice stays (see `noticeDuration`). */
  durationMs?: number;
}

/**
 * Turns an error notice into a more helpful one (plain-words explanation, one-click fixes);
 * registered by `features/recovery` so this store does not depend on it.
 */
type NoticeEnhancer = (notice: Notice) => Notice;
let enhancer: NoticeEnhancer | null = null;

export function setNoticeEnhancer(next: NoticeEnhancer | null) {
  enhancer = next;
}

/** Pending `git init` confirmation: for a folder that is not a repo, or a brand-new repo. */
export interface InitRequest {
  path: string;
  reason: "notRepository" | "new";
}

interface UiState {
  view: View;
  settingsOpen: boolean;
  resolvedTheme: ResolvedTheme;
  /** Phase 6.2: bumped whenever theme colors change (custom themes), so token readers repaint. */
  themeVersion: number;
  /** Run the scroll benchmark once the graph is first painted (`--bench-graph`). */
  benchOnLoad: boolean;
  notice: Notice | null;
  initRequest: InitRequest | null;
  setView: (view: View) => void;
  setSettingsOpen: (open: boolean) => void;
  showNotice: (notice: Notice | null) => void;
  requestInit: (request: InitRequest | null) => void;
}

export const useUi = create<UiState>()((set) => ({
  view: "workspace",
  settingsOpen: false,
  resolvedTheme: "light",
  themeVersion: 0,
  benchOnLoad: false,
  notice: null,
  initRequest: null,
  setView: (view) => set({ view }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  showNotice: (notice) =>
    set({ notice: notice && notice.kind === "error" && enhancer ? enhancer(notice) : notice }),
  requestInit: (initRequest) => set({ initRequest }),
}));
