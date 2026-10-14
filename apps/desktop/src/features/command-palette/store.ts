import { create } from "zustand";

/** Where the palette starts: the top level, or directly inside a command's sub-step. */
export interface PaletteStart {
  /** Id of a top-level command with `children` or `input` to open right away. */
  commandId?: string;
  query?: string;
}

interface PaletteState {
  open: boolean;
  start: PaletteStart | null;
  setOpen: (open: boolean) => void;
}

export const usePalette = create<PaletteState>()((set) => ({
  open: false,
  start: null,
  setOpen: (open) => set(open ? { open } : { open, start: null }),
}));

/** Opens the command palette, optionally inside a command (e.g. `"accounts.switch"`). */
export function openCommandPalette(start: PaletteStart | null = null) {
  usePalette.setState({ open: true, start });
}

export const closeCommandPalette = () => usePalette.getState().setOpen(false);

export function toggleCommandPalette() {
  if (usePalette.getState().open) closeCommandPalette();
  else openCommandPalette();
}

/** `Ctrl/Cmd+K` (layout independent). */
export const isPaletteShortcut = (e: KeyboardEvent) =>
  (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.code === "KeyK" || e.key === "k");

const RECENT_KEY = "nanomgg.palette.recent";
export const RECENT_LIMIT = 6;

/** Ids of recently run commands, newest first. */
export function readRecent(): string[] {
  try {
    const raw: unknown = JSON.parse(globalThis.localStorage?.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(raw) ? raw.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function pushRecent(id: string) {
  const next = [id, ...readRecent().filter((x) => x !== id)].slice(0, RECENT_LIMIT * 2);
  try {
    globalThis.localStorage?.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    // Storage full or unavailable: recents are a convenience only.
  }
}
