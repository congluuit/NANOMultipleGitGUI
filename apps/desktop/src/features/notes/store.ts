// Phase 6.5: view preferences of "My notes" (the notes themselves live in SQLite).
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { NotePriority } from "@/ipc";
import type { NoteSort } from "./model";

export type MyWorkTab = "work" | "notes";

interface NotesUi {
  /** Tab of the "My work" panel. */
  tab: MyWorkTab;
  query: string;
  tags: string[];
  priorities: NotePriority[];
  showDone: boolean;
  sort: NoteSort;
  selectedId: string | null;
  setTab: (tab: MyWorkTab) => void;
  setQuery: (query: string) => void;
  toggleTag: (id: string) => void;
  togglePriority: (p: NotePriority) => void;
  clearFilters: () => void;
  setShowDone: (on: boolean) => void;
  setSort: (sort: NoteSort) => void;
  select: (id: string | null) => void;
}

const toggle = <T>(list: readonly T[], item: T): T[] =>
  list.includes(item) ? list.filter((x) => x !== item) : [...list, item];

export const useNotesUi = create<NotesUi>()(
  persist(
    (set) => ({
      tab: "work",
      query: "",
      tags: [],
      priorities: [],
      showDone: true,
      sort: "priority",
      selectedId: null,
      setTab: (tab) => set({ tab }),
      setQuery: (query) => set({ query }),
      toggleTag: (id) => set((s) => ({ tags: toggle(s.tags, id) })),
      togglePriority: (p) => set((s) => ({ priorities: toggle(s.priorities, p) })),
      clearFilters: () => set({ query: "", tags: [], priorities: [] }),
      setShowDone: (showDone) => set({ showDone }),
      setSort: (sort) => set({ sort }),
      select: (selectedId) => set({ selectedId }),
    }),
    {
      name: "nanomgg.notesUi",
      version: 1,
      partialize: (s) => ({ tab: s.tab, showDone: s.showDone, sort: s.sort }),
    },
  ),
);
