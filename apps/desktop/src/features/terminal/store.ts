import { create } from "zustand";
import { persist } from "zustand/middleware";

/** One terminal tab. The shell session itself lives in `TerminalView` (backend id). */
export interface TerminalTab {
  key: string;
  repoId: number;
  /** Shell name reported by the backend once the session is open ("Git Bash", "zsh", …). */
  title: string;
}

export const MIN_HEIGHT = 120;
export const MAX_HEIGHT = 1600;
export const DEFAULT_HEIGHT = 260;

export const clampHeight = (height: number) =>
  Math.round(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, height)));

interface TerminalState {
  /** Panel visible. Hiding it keeps the shells running. */
  open: boolean;
  /** Panel height in px (persisted). */
  height: number;
  /** Tabs of every repo, in creation order. */
  tabs: TerminalTab[];
  /** Selected tab per repo id. */
  active: Record<number, string>;
  seq: number;
  /** Shows/hides the panel; opening it for a repo without terminals starts one. */
  toggle: (repoId: number) => void;
  setOpen: (open: boolean) => void;
  setHeight: (height: number) => void;
  addTab: (repoId: number) => string;
  /** Closes a tab (its session is closed by the view); hides the panel after the repo's last. */
  closeTab: (key: string) => void;
  activate: (repoId: number, key: string) => void;
  setTitle: (key: string, title: string) => void;
  /** Drops the terminals of repos that are no longer open. */
  retainRepos: (repoIds: number[]) => void;
}

export const useTerminal = create<TerminalState>()(
  persist(
    (set, get) => ({
      open: false,
      height: DEFAULT_HEIGHT,
      tabs: [],
      active: {},
      seq: 0,
      toggle: (repoId) => {
        if (get().open) {
          set({ open: false });
          return;
        }
        if (!get().tabs.some((t) => t.repoId === repoId)) get().addTab(repoId);
        set({ open: true });
      },
      setOpen: (open) => set({ open }),
      setHeight: (height) => set({ height: clampHeight(height) }),
      addTab: (repoId) => {
        const seq = get().seq + 1;
        const key = `term-${seq}`;
        set((s) => ({
          seq,
          open: true,
          tabs: [...s.tabs, { key, repoId, title: "" }],
          active: { ...s.active, [repoId]: key },
        }));
        return key;
      },
      closeTab: (key) =>
        set((s) => {
          const index = s.tabs.findIndex((t) => t.key === key);
          const tab = s.tabs[index];
          if (!tab) return s;
          const tabs = s.tabs.filter((t) => t.key !== key);
          const siblings = tabs.filter((t) => t.repoId === tab.repoId);
          const active = { ...s.active };
          if (siblings.length === 0) {
            delete active[tab.repoId];
            return { tabs, active, open: false };
          }
          if (active[tab.repoId] === key) {
            // Select the neighbour that took the closed tab's place (or the new last one).
            const before = s.tabs.slice(0, index).filter((t) => t.repoId === tab.repoId).length;
            active[tab.repoId] = (siblings[before] ?? siblings[siblings.length - 1])!.key;
          }
          return { tabs, active };
        }),
      activate: (repoId, key) => set((s) => ({ active: { ...s.active, [repoId]: key } })),
      setTitle: (key, title) =>
        set((s) => ({ tabs: s.tabs.map((t) => (t.key === key ? { ...t, title } : t)) })),
      retainRepos: (repoIds) =>
        set((s) => {
          const keep = new Set(repoIds);
          if (s.tabs.every((t) => keep.has(t.repoId))) return s;
          const active: Record<number, string> = {};
          for (const [id, key] of Object.entries(s.active)) {
            if (keep.has(Number(id))) active[Number(id)] = key;
          }
          return { tabs: s.tabs.filter((t) => keep.has(t.repoId)), active };
        }),
    }),
    {
      name: "nanomgg.terminal",
      version: 1,
      // Only the layout survives a restart; shells never do.
      partialize: (s) => ({ height: s.height }),
    },
  ),
);
