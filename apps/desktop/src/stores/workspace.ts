import { create } from "zustand";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type RepoTab, type WorkspaceState } from "@/ipc";

/** How many closed tabs "Reopen closed tab" remembers (this session only). */
export const RECENTLY_CLOSED_LIMIT = 20;

interface WorkspaceStore {
  tabs: RepoTab[];
  activeId: number | null;
  loaded: boolean;
  /** Tabs closed in this session, newest first (gone again once reopened or forgotten). */
  recentlyClosed: RepoTab[];
  load: () => Promise<void>;
  open: (path: string) => Promise<RepoTab>;
  init: (path: string, initialBranch: string) => Promise<RepoTab>;
  close: (id: number) => Promise<void>;
  forget: (id: number) => Promise<void>;
  setActive: (id: number) => void;
  /** New tab order (drag and drop); saved by the backend. */
  reorder: (ids: number[]) => Promise<void>;
}

function upsertTab(tabs: RepoTab[], tab: RepoTab): RepoTab[] {
  return tabs.some((t) => t.id === tab.id)
    ? tabs.map((t) => (t.id === tab.id ? tab : t))
    : [...tabs, tab];
}

const without = (tabs: RepoTab[], id: number) => tabs.filter((t) => t.id !== id);

export const useWorkspace = create<WorkspaceStore>()((set, get) => {
  const apply = (state: WorkspaceState) => set({ tabs: state.tabs, activeId: state.activeId });
  return {
    tabs: [],
    activeId: null,
    loaded: false,
    recentlyClosed: [],
    load: async () => {
      if (!isTauri()) {
        set({ loaded: true });
        return;
      }
      try {
        apply(await commands.workspaceState());
      } finally {
        set({ loaded: true });
      }
    },
    open: async (path) => {
      const tab = await commands.repoOpen(path);
      set((s) => ({
        tabs: upsertTab(s.tabs, tab),
        activeId: tab.id,
        recentlyClosed: without(s.recentlyClosed, tab.id),
      }));
      return tab;
    },
    init: async (path, initialBranch) => {
      const tab = await commands.repoInit(path, initialBranch);
      set((s) => ({
        tabs: upsertTab(s.tabs, tab),
        activeId: tab.id,
        recentlyClosed: without(s.recentlyClosed, tab.id),
      }));
      return tab;
    },
    close: async (id) => {
      const closed = get().tabs.find((t) => t.id === id);
      apply(await commands.repoClose(id));
      if (closed) {
        set((s) => ({
          recentlyClosed: [closed, ...without(s.recentlyClosed, id)].slice(
            0,
            RECENTLY_CLOSED_LIMIT,
          ),
        }));
      }
    },
    forget: async (id) => {
      apply(await commands.recentRemove(id));
      set((s) => ({ recentlyClosed: without(s.recentlyClosed, id) }));
    },
    reorder: async (ids) => {
      const byId = new Map(get().tabs.map((t) => [t.id, t]));
      // Optimistic: the strip moves at once, the backend answer then confirms it.
      set({ tabs: ids.flatMap((id) => byId.get(id) ?? []) });
      if (!isTauri()) return;
      const state = await commands.workspaceReorder(ids);
      // The dev mock backend answers unknown commands with `null`.
      if (state) apply(state);
    },
    setActive: (id) => {
      set({ activeId: id });
      if (isTauri()) {
        commands.workspaceSetActive(id).catch((e: unknown) => console.error(e));
      }
    },
  };
});

export const useActiveRepo = () =>
  useWorkspace((s) => s.tabs.find((t) => t.id === s.activeId) ?? null);
