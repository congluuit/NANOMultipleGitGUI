export type TokenMap = Record<string, string>;

/** Parses `tokens.css` into `{ light, dark }` maps of `--name -> value`. */
export function parseTokensCss(css: string): { light: TokenMap; dark: TokenMap } {
  const block = (selector: RegExp): TokenMap => {
    const match = selector.exec(css);
    if (!match?.[1]) throw new Error(`Token block not found: ${selector}`);
    const map: TokenMap = {};
    for (const m of match[1].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
      if (m[1] && m[2]) map[m[1]] = m[2].trim();
    }
    return map;
  };
  return {
    light: block(/\[data-theme="light"\]\s*\{([^}]*)\}/),
    dark: block(/\[data-theme="dark"\]\s*\{([^}]*)\}/),
  };
}

export const COLOR_TOKENS = [
  "--bg",
  "--surface",
  "--surface-2",
  "--border",
  "--text",
  "--text-muted",
  "--primary",
  "--primary-hover",
  "--primary-fg",
  "--success",
  "--warning",
  "--danger",
  "--info",
] as const;

export const LANE_TOKENS = Array.from({ length: 8 }, (_, i) => `--lane-${i}` as const);
