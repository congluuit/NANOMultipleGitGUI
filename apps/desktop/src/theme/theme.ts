import { expandHex } from "./color";

export type ThemeMode = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export const THEME_MODES: readonly ThemeMode[] = ["light", "dark", "system"];

const darkQuery = "(prefers-color-scheme: dark)";

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.(darkQuery).matches;
}

export function resolveTheme(mode: ThemeMode, systemDark: boolean): ResolvedTheme {
  if (mode === "system") return systemDark ? "dark" : "light";
  return mode;
}

export function applyTheme(theme: ResolvedTheme, root: HTMLElement = document.documentElement) {
  root.dataset.theme = theme;
}

/** Calls `onChange` whenever the OS color scheme changes; returns an unsubscribe function. */
export function watchSystemTheme(onChange: (dark: boolean) => void): () => void {
  const mql = window.matchMedia?.(darkQuery);
  if (!mql) return () => {};
  const listener = (e: MediaQueryListEvent) => onChange(e.matches);
  mql.addEventListener("change", listener);
  return () => mql.removeEventListener("change", listener);
}

/** Reads a token (e.g. `--primary`) as resolved on `el`; used by canvas/Monaco/xterm renderers. */
export function readToken(name: `--${string}`, el: Element = document.documentElement): string {
  return expandHex(getComputedStyle(el).getPropertyValue(name).trim());
}
