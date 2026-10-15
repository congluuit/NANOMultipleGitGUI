// Phase 6.2: custom themes — presets, token groups, contrast checks (pure logic).
import type { CustomTheme } from "@/ipc";
import { expandHex } from "@/theme/color";
import { AA_NON_TEXT, CONTRAST_RULES, contrastRatio, type ContrastRule } from "@/theme/contrast";
import type { ResolvedTheme } from "@/theme/theme";
import { COLOR_TOKENS, LANE_TOKENS } from "@/theme/tokens";

export const THEME_TOKENS = [...COLOR_TOKENS, ...LANE_TOKENS] as const;
export type ThemeToken = (typeof THEME_TOKENS)[number];
export type ThemeColors = Record<ThemeToken, string>;

export type PresetId = "hc-dark" | "hc-light";
export const PRESET_IDS: readonly PresetId[] = ["hc-dark", "hc-light"];

/** A theme ready to show or apply: presets and stored themes alike, every token filled in. */
export interface ThemeDef {
  id: string;
  /** User themes only; presets are named with `presetName()`. */
  name: string;
  base: ResolvedTheme;
  colors: ThemeColors;
  preset: boolean;
}

function complete(map: Record<string, string>, fallback?: ThemeColors): ThemeColors {
  const out = {} as ThemeColors;
  for (const token of THEME_TOKENS) {
    const value = normalizeHex(map[token] ?? "") ?? fallback?.[token];
    if (!value) throw new Error(`Missing token ${token}`);
    out[token] = value;
  }
  return out;
}

/**
 * The built-in light / dark palettes: a copy of `tokens.css` (the CSS stays the source of truth;
 * `model.test.ts` fails when the two drift apart).
 */
export const BASE_COLORS: Record<ResolvedTheme, ThemeColors> = {
  light: complete({
    "--bg": "#faf8f5",
    "--surface": "#ffffff",
    "--surface-2": "#f2eee8",
    "--border": "#e4ded5",
    "--text": "#1f1b16",
    "--text-muted": "#6b635a",
    "--primary": "#c2410c",
    "--primary-hover": "#a63e00",
    "--primary-fg": "#ffffff",
    "--success": "#237a35",
    "--warning": "#8f5b00",
    "--danger": "#c92a2a",
    "--info": "#1a68b0",
    "--lane-0": "#d9480f",
    "--lane-1": "#1971c2",
    "--lane-2": "#7048e8",
    "--lane-3": "#2f9e44",
    "--lane-4": "#c2255c",
    "--lane-5": "#b35f00",
    "--lane-6": "#5c940d",
    "--lane-7": "#c92a2a",
  }),
  dark: complete({
    "--bg": "#16130f",
    "--surface": "#1f1b17",
    "--surface-2": "#2a251f",
    "--border": "#3a332b",
    "--text": "#f1ece4",
    "--text-muted": "#a39a8f",
    "--primary": "#ff8a3d",
    "--primary-hover": "#ffa463",
    "--primary-fg": "#1f1b16",
    "--success": "#51cf66",
    "--warning": "#fcc419",
    "--danger": "#ff6b6b",
    "--info": "#4dabf7",
    "--lane-0": "#ff8a3d",
    "--lane-1": "#4dabf7",
    "--lane-2": "#b197fc",
    "--lane-3": "#69db7c",
    "--lane-4": "#f783ac",
    "--lane-5": "#ffd43b",
    "--lane-6": "#a9e34b",
    "--lane-7": "#e8590c",
  }),
};

/**
 * High-contrast presets: pure black / white backgrounds, clear borders, bright accents. Text,
 * accent and status colors reach WCAG AAA (≥ 7:1) on the backgrounds (`contrast.test.ts`).
 */
export const PRESETS: Record<PresetId, { base: ResolvedTheme; colors: ThemeColors }> = {
  "hc-dark": {
    base: "dark",
    colors: {
      "--bg": "#000000",
      "--surface": "#000000",
      "--surface-2": "#1c1c1c",
      "--border": "#a6a6a6",
      "--text": "#ffffff",
      "--text-muted": "#d9d9d9",
      "--primary": "#ffd23f",
      "--primary-hover": "#ffe580",
      "--primary-fg": "#000000",
      "--success": "#5cff7a",
      "--warning": "#ffb84d",
      "--danger": "#ff8080",
      "--info": "#7fd4ff",
      "--lane-0": "#ffb84d",
      "--lane-1": "#7fd4ff",
      "--lane-2": "#d2b4ff",
      "--lane-3": "#5cff7a",
      "--lane-4": "#ff9ccf",
      "--lane-5": "#ffe14d",
      "--lane-6": "#c4f55c",
      "--lane-7": "#ff8080",
    },
  },
  "hc-light": {
    base: "light",
    colors: {
      "--bg": "#ffffff",
      "--surface": "#ffffff",
      "--surface-2": "#ebebeb",
      "--border": "#4d4d4d",
      "--text": "#000000",
      "--text-muted": "#262626",
      "--primary": "#0b3d91",
      "--primary-hover": "#072a66",
      "--primary-fg": "#ffffff",
      "--success": "#05570f",
      "--warning": "#6b3a00",
      "--danger": "#9b0000",
      "--info": "#00467f",
      "--lane-0": "#a33a00",
      "--lane-1": "#0050a0",
      "--lane-2": "#5a1fb0",
      "--lane-3": "#1b6b1b",
      "--lane-4": "#a0124e",
      "--lane-5": "#7a5000",
      "--lane-6": "#3d6600",
      "--lane-7": "#a10000",
    },
  },
};

export function isPresetId(id: string | null | undefined): id is PresetId {
  return id === "hc-dark" || id === "hc-light";
}

/** `#rgb` / `#rrggbb` (any case, spaces around) → lowercase `#rrggbb`; `null` otherwise. */
export function normalizeHex(value: string): string | null {
  let v = value.trim();
  if (!v.startsWith("#")) v = `#${v}`;
  v = expandHex(v).toLowerCase();
  return /^#[0-9a-f]{6}$/.test(v) ? v : null;
}

/** A stored theme with missing tokens filled from its base (older / hand-edited files). */
export function fromStored(theme: CustomTheme): ThemeDef {
  return {
    id: theme.id,
    name: theme.name,
    base: theme.base,
    colors: complete(theme.colors, BASE_COLORS[theme.base]),
    preset: false,
  };
}

export function presetDef(id: PresetId): ThemeDef {
  return { id, name: "", base: PRESETS[id].base, colors: { ...PRESETS[id].colors }, preset: true };
}

/** Preset or stored theme by id; `null` when it does not exist (any more). */
export function findTheme(id: string | null, themes: readonly CustomTheme[]): ThemeDef | null {
  if (!id) return null;
  if (isPresetId(id)) return presetDef(id);
  const stored = themes.find((t) => t.id === id);
  return stored ? fromStored(stored) : null;
}

/** Theme picked when the user switches to "Custom" without having chosen one yet. */
export function defaultCustomId(themes: readonly CustomTheme[], activeId: string | null): string {
  if (findTheme(activeId, themes)) return activeId!;
  return themes[0]?.id ?? "hc-dark";
}

/** Starting points offered by "New theme". */
export type ThemeSource = "light" | "dark" | PresetId | { copyOf: string };

export function sourceColors(
  source: ThemeSource,
  themes: readonly CustomTheme[],
): { base: ResolvedTheme; colors: ThemeColors } {
  if (source === "light" || source === "dark") {
    return { base: source, colors: { ...BASE_COLORS[source] } };
  }
  if (typeof source === "string") return presetDef(source);
  const def = findTheme(source.copyOf, themes);
  return def ? { base: def.base, colors: { ...def.colors } } : sourceColors("dark", themes);
}

/** Name not used yet: "Name", "Name (2)", … (case-insensitive, like the backend). */
export function uniqueName(name: string, taken: readonly string[]): string {
  const used = new Set(taken.map((n) => n.toLowerCase()));
  if (!used.has(name.toLowerCase())) return name;
  for (let i = 2; ; i++) {
    const candidate = `${name} (${i})`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
}

export type TokenGroup = "background" | "text" | "accent" | "status" | "lanes";

/** Editor sections, in display order. */
export const TOKEN_GROUPS: readonly { id: TokenGroup; tokens: readonly ThemeToken[] }[] = [
  { id: "background", tokens: ["--bg", "--surface", "--surface-2", "--border"] },
  { id: "text", tokens: ["--text", "--text-muted"] },
  { id: "accent", tokens: ["--primary", "--primary-hover", "--primary-fg"] },
  { id: "status", tokens: ["--success", "--warning", "--danger", "--info"] },
  { id: "lanes", tokens: LANE_TOKENS },
];

type TokenName =
  | "bg"
  | "surface"
  | "surface-2"
  | "border"
  | "text"
  | "text-muted"
  | "primary"
  | "primary-hover"
  | "primary-fg"
  | "success"
  | "warning"
  | "danger"
  | "info"
  | `lane-${0 | 1 | 2 | 3 | 4 | 5 | 6 | 7}`;

/** i18n key suffix of a token: `--surface-2` → `surface-2`. */
export function tokenKey(token: ThemeToken): TokenName {
  return token.slice(2) as TokenName;
}

export interface ContrastIssue extends ContrastRule {
  ratio: number;
  /** Graphical objects (lane colors) rather than text. */
  graphic: boolean;
}

/** Pairs below the app's minimum contrast (same rules as the built-in themes). */
export function contrastIssues(colors: ThemeColors): ContrastIssue[] {
  const out: ContrastIssue[] = [];
  for (const rule of CONTRAST_RULES) {
    const fg = colors[rule.fg as ThemeToken];
    const bg = colors[rule.bg as ThemeToken];
    if (!fg || !bg) continue;
    const ratio = contrastRatio(fg, bg);
    if (ratio < rule.min) {
      out.push({ ...rule, ratio, graphic: rule.fg.startsWith("--lane-") });
    }
  }
  return out;
}

/** Ratio shown to people: one decimal, rounded down so "4.5" is never shown for 4.47. */
export function formatRatio(ratio: number): string {
  return (Math.floor(ratio * 10) / 10).toFixed(1);
}

/** WCAG AAA (7:1) rules the high-contrast presets meet, on top of `CONTRAST_RULES`. */
export const AAA_TEXT = 7;
export const HIGH_CONTRAST_RULES: readonly ContrastRule[] = [
  ...["--bg", "--surface", "--surface-2"].flatMap((bg) => [
    { fg: "--text", bg, min: AAA_TEXT },
    { fg: "--text-muted", bg, min: AAA_TEXT },
  ]),
  { fg: "--primary-fg", bg: "--primary", min: AAA_TEXT },
  { fg: "--primary-fg", bg: "--primary-hover", min: AAA_TEXT },
  ...["--primary", "--success", "--warning", "--danger", "--info"].flatMap((fg) => [
    { fg, bg: "--bg", min: AAA_TEXT },
    { fg, bg: "--surface", min: AAA_TEXT },
  ]),
  { fg: "--border", bg: "--bg", min: AA_NON_TEXT },
  { fg: "--border", bg: "--surface-2", min: AA_NON_TEXT },
  ...LANE_TOKENS.map((fg) => ({ fg, bg: "--surface", min: 4.5 })),
];
