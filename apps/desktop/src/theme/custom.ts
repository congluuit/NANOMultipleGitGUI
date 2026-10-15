// Phase 6.2: custom themes are applied as inline CSS variables on <html>, on top of the light or
// dark token block picked by `data-theme` (the theme's "base").
import type { ResolvedTheme } from "./theme";
import { COLOR_TOKENS, LANE_TOKENS } from "./tokens";

export interface AppliedCustomTheme {
  base: ResolvedTheme;
  /** `--token` → `#rrggbb`. */
  colors: Readonly<Record<string, string>>;
}

const CACHE_KEY = "nanomgg.customTheme";
const TOKENS: readonly string[] = [...COLOR_TOKENS, ...LANE_TOKENS];

/** Sets the theme's colors inline (or removes them all with `null`). */
export function applyCustomColors(
  colors: Readonly<Record<string, string>> | null,
  root: HTMLElement = document.documentElement,
) {
  for (const token of TOKENS) {
    const value = colors?.[token];
    if (value) root.style.setProperty(token, value);
    else root.style.removeProperty(token);
  }
}

/**
 * The active custom theme as last applied, kept in localStorage so the first paint already has
 * it (SQLite is read asynchronously, like the settings cache).
 */
export function readCustomCache(): AppliedCustomTheme | null {
  try {
    const raw = globalThis.localStorage?.getItem(CACHE_KEY);
    if (!raw) return null;
    const value = JSON.parse(raw) as Partial<AppliedCustomTheme>;
    if ((value.base !== "light" && value.base !== "dark") || typeof value.colors !== "object") {
      return null;
    }
    const colors: Record<string, string> = {};
    for (const [k, v] of Object.entries(value.colors ?? {})) {
      if (TOKENS.includes(k) && typeof v === "string" && /^#[0-9a-f]{6}$/i.test(v)) colors[k] = v;
    }
    return { base: value.base, colors };
  } catch {
    return null;
  }
}

export function writeCustomCache(value: AppliedCustomTheme | null) {
  try {
    if (value) globalThis.localStorage?.setItem(CACHE_KEY, JSON.stringify(value));
    else globalThis.localStorage?.removeItem(CACHE_KEY);
  } catch {
    // Storage full or disabled: only the first paint is affected.
  }
}
