/**
 * Quick open (Ctrl+P): fuzzy matching of repository paths. The file name counts more than the
 * folders, so "apsh" finds `features/shell/AppShell.tsx` first. `path:line[:column]` jumps to
 * a line.
 */
import { create } from "zustand";
import { normalize, scoreToken } from "@/features/command-palette/fuzzy";

export const useQuickOpen = create<{ open: boolean }>()(() => ({ open: false }));

export const openQuickOpen = () => useQuickOpen.setState({ open: true });

/** `Ctrl/Cmd+P` (layout independent). */
export const isQuickOpenShortcut = (e: KeyboardEvent) =>
  (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.code === "KeyP" || e.key === "p");

export interface FileQuery {
  text: string;
  line: number | null;
  column: number | null;
}

export function parseFileQuery(raw: string): FileQuery {
  const m = /^(.*?)(?::(\d+))?(?::(\d+))?$/.exec(raw.trim().replaceAll("\\", "/"));
  return {
    text: m?.[1] ?? "",
    line: m?.[2] ? Number(m[2]) : null,
    column: m?.[3] ? Number(m[3]) : null,
  };
}

export interface PreparedPath {
  path: string;
  norm: string;
  base: string;
}

export function preparePaths(paths: readonly string[]): PreparedPath[] {
  return paths.map((path) => {
    const norm = normalize(path);
    return { path, norm, base: norm.slice(norm.lastIndexOf("/") + 1) };
  });
}

/** A hit in the file name beats the same hit in a folder name. */
const NAME_BONUS = 300;

function scorePath(tokens: string[], p: PreparedPath): number | null {
  let total = 0;
  for (const token of tokens) {
    const inPath = scoreToken(token, p.norm);
    const inName = token.includes("/") ? null : scoreToken(token, p.base);
    if (inPath === null && inName === null) return null;
    total += Math.max(inPath ?? -Infinity, inName === null ? -Infinity : inName + NAME_BONUS);
  }
  // Shorter paths win ties.
  return total - p.path.length / 1000;
}

/** Best matching paths, best first. */
export function rankPaths(query: string, paths: readonly PreparedPath[], limit: number): string[] {
  const tokens = normalize(query).trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return paths.slice(0, limit).map((p) => p.path);
  const scored: { path: string; score: number }[] = [];
  for (const p of paths) {
    const score = scorePath(tokens, p);
    if (score !== null) scored.push({ path: p.path, score });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, limit).map((s) => s.path);
}
