// Phase 6.5: notes data (SQLite `notes.items` / `notes.tags`) through TanStack Query.
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type NoteInput, type NotesData } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const notesKey = ["notes"] as const;
/** Settings → Storage → "Your data in the app" (counts and sizes). */
export const userDataKey = ["userData", "summary"] as const;

const EMPTY: NotesData = { notes: [], tags: [] };

export function useNotes() {
  return useQuery({
    queryKey: notesKey,
    queryFn: () => commands.notesLoad(),
    enabled: isTauri(),
    staleTime: Infinity,
    placeholderData: EMPTY,
  });
}

/** Stores what a command returned (every notes command returns the full data). */
export function setNotes(data: NotesData): NotesData {
  queryClient.setQueryData(notesKey, data);
  void queryClient.invalidateQueries({ queryKey: userDataKey });
  return data;
}

export const saveNote = async (input: NoteInput) => setNotes(await commands.notesSave(input));
export const deleteNotes = async (ids: string[]) => setNotes(await commands.notesDelete(ids));
