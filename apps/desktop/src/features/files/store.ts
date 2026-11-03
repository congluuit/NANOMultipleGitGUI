import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { SearchOptions } from "@/ipc";
import { useLayout } from "@/stores/layout";
import { ancestors } from "./tree";

/** What the left sidebar shows: branches and refs, the file tree, or the search panel. */
export type SidebarView = "refs" | "files" | "search";

interface SidebarViewState {
  view: SidebarView;
  /** Bumped to move the focus into the view's input (Ctrl+Shift+F). */
  focusTick: number;
  setView: (view: SidebarView) => void;
}

export const useSidebarView = create<SidebarViewState>()(
  persist(
    (set) => ({
      view: "refs",
      focusTick: 0,
      setView: (view) => set({ view }),
    }),
    {
      name: "nanomgg.sidebarView",
      version: 1,
      partialize: (s) => ({ view: s.view }),
    },
  ),
);

/** Shows a view of the sidebar (expanding it when collapsed) and focuses its input. */
export function showSidebarView(view: SidebarView) {
  useSidebarView.setState((s) => ({ view, focusTick: s.focusTick + 1 }));
  const layout = useLayout.getState();
  if (layout.sidebarCollapsed) layout.setCollapsed("sidebar", false);
}

/** `Ctrl/Cmd+Shift+F`: search in the repository's files. */
export const isSearchShortcut = (e: KeyboardEvent) =>
  (e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && (e.code === "KeyF" || e.key === "F");

interface ExpandedState {
  /** Expanded folders of the file tree, per repository. */
  expanded: Record<number, string[]>;
  toggle: (repoId: number, folder: string) => void;
  /** Expands every folder above `path`. */
  reveal: (repoId: number, path: string) => void;
  collapseAll: (repoId: number) => void;
}

export const useExpandedFolders = create<ExpandedState>()((set) => ({
  expanded: {},
  toggle: (repoId, folder) =>
    set((s) => {
      const list = s.expanded[repoId] ?? [];
      const next = list.includes(folder) ? list.filter((f) => f !== folder) : [...list, folder];
      return { expanded: { ...s.expanded, [repoId]: next } };
    }),
  reveal: (repoId, path) =>
    set((s) => {
      const list = new Set(s.expanded[repoId] ?? []);
      for (const a of ancestors(path)) list.add(a);
      return { expanded: { ...s.expanded, [repoId]: [...list] } };
    }),
  collapseAll: (repoId) => set((s) => ({ expanded: { ...s.expanded, [repoId]: [] } })),
}));

/** A create / rename / delete dialog of the file tree. */
export type FileDialog =
  | { kind: "newFile" | "newFolder"; repoId: number; parent: string }
  | { kind: "rename"; repoId: number; path: string; folder: boolean }
  | { kind: "delete"; repoId: number; path: string; folder: boolean; untracked: boolean };

export const useFileDialog = create<{ dialog: FileDialog | null }>()(() => ({ dialog: null }));

export const openFileDialog = (dialog: FileDialog | null) => useFileDialog.setState({ dialog });

/** The search panel keeps its query and results while the sidebar shows something else. */
interface SearchState {
  query: string;
  options: SearchOptions;
  setQuery: (query: string) => void;
  setOption: (key: keyof SearchOptions, value: boolean) => void;
}

export const useSearchState = create<SearchState>()((set) => ({
  query: "",
  options: { caseSensitive: false, wholeWord: false, regex: false, untracked: true },
  setQuery: (query) => set({ query }),
  setOption: (key, value) => set((s) => ({ options: { ...s.options, [key]: value } })),
}));
