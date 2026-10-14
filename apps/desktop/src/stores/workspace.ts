import { create } from "zustand";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type RepoTab, type WorkspaceState } from "@/ipc";

interface WorkspaceStore {
  tabs: RepoTab[];
  activeId: number | null;
  loaded: boolean;
  load: () => Promise<void>;
  open: (path: string) => Promise<RepoTab>;
  init: (path: string, initialBranch: string) => Promise<RepoTab>;
  close: (id: number) => Promise<void>;
  forget: (id: number) => Promise<void>;
  setActive: (id: number) => void;
}

function upsertTab(tabs: RepoTab[], tab: RepoTab): RepoTab[] {
  return tabs.some((t) => t.id === tab.id)
    ? tabs.map((t) => (t.id === tab.id ? tab : t))
    : [...tabs, tab];
}

export const useWorkspace = create<WorkspaceStore>()((set) => {
  const apply = (state: WorkspaceState) => set({ tabs: state.tabs, activeId: state.activeId });
  return {
    tabs: [],
    activeId: null,
    loaded: false,
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
      set((s) => ({ tabs: upsertTab(s.tabs, tab), activeId: tab.id }));
      return tab;
    },
    init: async (path, initialBranch) => {
      const tab = await commands.repoInit(path, initialBranch);
      set((s) => ({ tabs: upsertTab(s.tabs, tab), activeId: tab.id }));
      return tab;
    },
    close: async (id) => apply(await commands.repoClose(id)),
    forget: async (id) => apply(await commands.recentRemove(id)),
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
