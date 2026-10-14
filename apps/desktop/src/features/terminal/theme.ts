import type { ITheme } from "@xterm/xterm";
import type { ResolvedTheme } from "@/theme/theme";
import { readToken } from "@/theme/theme";

type Read = (name: `--${string}`) => string;

/** `#rrggbb` + alpha (0–1) → `#rrggbbaa`; other formats are returned unchanged. */
export function withAlpha(hex: string, alpha: number): string {
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0");
  return /^#[0-9a-f]{6}$/i.test(hex) ? `${hex}${a}` : hex;
}

/**
 * xterm.js theme built from the design tokens (PLAN.md §7.2: Monaco, xterm and the graph read the
 * same tokens). ANSI colors reuse the semantic and graph-lane tokens so `git status`/`git log`
 * colors match the rest of the app.
 */
export function xtermTheme(theme: ResolvedTheme, read: Read = readToken): ITheme {
  const text = read("--text");
  const muted = read("--text-muted");
  const surface = read("--surface");
  const primary = read("--primary");
  const dark = theme === "dark";
  return {
    background: surface,
    foreground: text,
    cursor: primary,
    cursorAccent: surface,
    selectionBackground: withAlpha(primary, 0.3),
    selectionInactiveBackground: withAlpha(muted, 0.25),
    scrollbarSliderBackground: withAlpha(muted, 0.25),
    scrollbarSliderHoverBackground: withAlpha(muted, 0.4),
    scrollbarSliderActiveBackground: withAlpha(muted, 0.5),
    black: dark ? read("--border") : text,
    red: read("--danger"),
    green: read("--success"),
    yellow: read("--warning"),
    blue: read("--lane-1"),
    magenta: read("--lane-2"),
    cyan: read("--info"),
    white: dark ? text : muted,
    brightBlack: muted,
    brightRed: read("--lane-7"),
    brightGreen: read("--lane-3"),
    brightYellow: read("--lane-5"),
    brightBlue: read("--lane-1"),
    brightMagenta: read("--lane-4"),
    brightCyan: read("--info"),
    brightWhite: dark ? text : muted,
  };
}

const FALLBACK_FONT =
  '"Cascadia Code", "JetBrains Mono", "SF Mono", ui-monospace, Consolas, monospace';

/** Monospace stack from `--font-mono` (same stack as `index.css` if the variable is missing). */
export function terminalFont(read: Read = readToken): string {
  return read("--font-mono") || FALLBACK_FONT;
}
