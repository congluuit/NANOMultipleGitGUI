import { expandHex } from "./color";

/**
 * WCAG 2.1 relative luminance of a `#rrggbb` color. `#rgb` is accepted too: the CSS minifier
 * of release builds shortens token values such as `#ffffff` to `#fff`.
 */
export function relativeLuminance(hex: string): number {
  const m = /^#([0-9a-f]{6})$/i.exec(expandHex(hex.trim()));
  if (!m?.[1]) throw new Error(`Expected #rrggbb color, got "${hex}"`);
  const value = m[1];
  const channel = (i: number) => {
    const c = parseInt(value.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}

/** WCAG 2.1 contrast ratio between two `#rrggbb` colors (1–21). */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** Minimum ratios: normal text (AA) and UI components / graphical objects (WCAG 1.4.11). */
export const AA_TEXT = 4.5;
export const AA_NON_TEXT = 3;

export interface ContrastRule {
  fg: string;
  bg: string;
  min: number;
}

/** Token pairs every theme must satisfy. Shared by the unit test and the palette page. */
export const CONTRAST_RULES: readonly ContrastRule[] = [
  ...["--bg", "--surface", "--surface-2"].flatMap((bg) => [
    { fg: "--text", bg, min: AA_TEXT },
    { fg: "--text-muted", bg, min: AA_TEXT },
  ]),
  { fg: "--primary-fg", bg: "--primary", min: AA_TEXT },
  { fg: "--primary-fg", bg: "--primary-hover", min: AA_TEXT },
  ...["--primary", "--success", "--warning", "--danger", "--info"].flatMap((fg) => [
    { fg, bg: "--bg", min: AA_TEXT },
    { fg, bg: "--surface", min: AA_TEXT },
    { fg, bg: "--surface-2", min: AA_NON_TEXT },
  ]),
  ...Array.from({ length: 8 }, (_, i) => ({
    fg: `--lane-${i}`,
    bg: "--surface",
    min: AA_NON_TEXT,
  })),
];
