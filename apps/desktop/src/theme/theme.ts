import { expandHex } from "./color";

/** `custom` (Phase 6.2): a preset or user theme, see `features/custom-theme`. */
export type ThemeMode = "light" | "dark" | "system" | "custom";
export type ResolvedTheme = "light" | "dark";

/** Modes with a built-in palette (`custom` also needs a theme picked). */
export const THEME_MODES = ["light", "dark", "system"] as const;

const darkQuery = "(prefers-color-scheme: dark)";

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.(darkQuery).matches;
}

/** `custom` resolves to the base of its theme, or follows the OS while no theme is known. */
export function resolveTheme(
  mode: ThemeMode,
  systemDark: boolean,
  customBase?: ResolvedTheme | null,
): ResolvedTheme {
  if (mode === "custom" && customBase) return customBase;
  if (mode === "system" || mode === "custom") return systemDark ? "dark" : "light";
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
