// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { CONTRAST_RULES, contrastRatio } from "@/theme/contrast";
import { parseTokensCss } from "@/theme/tokens";
import {
  BASE_COLORS,
  contrastIssues,
  defaultCustomId,
  findTheme,
  formatRatio,
  fromStored,
  HIGH_CONTRAST_RULES,
  normalizeHex,
  PRESET_IDS,
  PRESETS,
  sourceColors,
  THEME_TOKENS,
  uniqueName,
} from "./model";
import type { CustomTheme } from "@/ipc";

const stored = (over: Partial<CustomTheme> = {}): CustomTheme => ({
  id: "theme_1",
  name: "Mine",
  base: "dark",
  colors: { "--bg": "#101010" },
  createdAt: 1,
  updatedAt: 1,
  ...over,
});

describe.each(PRESET_IDS)("%s preset", (id) => {
  const colors = PRESETS[id].colors;

  it("defines every token as #rrggbb", () => {
    for (const token of THEME_TOKENS) expect(colors[token], token).toMatch(/^#[0-9a-f]{6}$/);
  });

  it.each(
    [...CONTRAST_RULES, ...HIGH_CONTRAST_RULES].map(
      (r) => [`${r.fg} on ${r.bg} ≥ ${r.min}`, r] as const,
    ),
  )("%s", (_label, rule) => {
    const fg = colors[rule.fg as keyof typeof colors];
    const bg = colors[rule.bg as keyof typeof colors];
    expect(contrastRatio(fg!, bg!)).toBeGreaterThanOrEqual(rule.min);
  });

  it("reports no contrast issue", () => {
    expect(contrastIssues(colors)).toEqual([]);
  });
});

describe("base palettes", () => {
  it("match tokens.css and pass the app's contrast rules", () => {
    const css = readFileSync(new URL("../../theme/tokens.css", import.meta.url), "utf8");
    const parsed = parseTokensCss(css);
    for (const base of ["light", "dark"] as const) {
      for (const token of THEME_TOKENS) {
        expect(BASE_COLORS[base][token], `${base} ${token}`).toBe(parsed[base][token]);
      }
    }
    expect(contrastIssues(BASE_COLORS.light)).toEqual([]);
    expect(contrastIssues(BASE_COLORS.dark)).toEqual([]);
  });
});

describe("contrastIssues", () => {
  it("flags unreadable text with its ratio", () => {
    const colors = { ...BASE_COLORS.light, "--text-muted": "#bbbbbb" };
    const issues = contrastIssues(colors);
    expect(issues.length).toBeGreaterThan(0);
    const onBg = issues.find((i) => i.fg === "--text-muted" && i.bg === "--bg")!;
    expect(onBg.ratio).toBeLessThan(4.5);
    expect(onBg.graphic).toBe(false);
    expect(formatRatio(onBg.ratio)).toMatch(/^\d+\.\d$/);
  });

  it("marks lane colors as graphics", () => {
    const issues = contrastIssues({ ...BASE_COLORS.light, "--lane-3": "#f5f5f5" });
    expect(issues).toEqual([expect.objectContaining({ fg: "--lane-3", graphic: true })]);
  });
});

describe("helpers", () => {
  it("normalizes hex colors", () => {
    expect(normalizeHex("#ABC")).toBe("#aabbcc");
    expect(normalizeHex(" 1a2B3c ")).toBe("#1a2b3c");
    expect(normalizeHex("#12345")).toBeNull();
    expect(normalizeHex("red")).toBeNull();
  });

  it("rounds ratios down so 4.47 is not shown as 4.5", () => {
    expect(formatRatio(4.47)).toBe("4.4");
    expect(formatRatio(7)).toBe("7.0");
  });

  it("fills tokens missing from a stored theme with its base", () => {
    const def = fromStored(stored());
    expect(def.colors["--bg"]).toBe("#101010");
    expect(def.colors["--text"]).toBe(BASE_COLORS.dark["--text"]);
    expect(def.preset).toBe(false);
  });

  it("finds presets and stored themes, null for unknown ids", () => {
    const themes = [stored()];
    expect(findTheme("hc-dark", themes)?.preset).toBe(true);
    expect(findTheme("theme_1", themes)?.name).toBe("Mine");
    expect(findTheme("theme_gone", themes)).toBeNull();
    expect(findTheme(null, themes)).toBeNull();
  });

  it("keeps a valid selection, otherwise picks the first own theme or a preset", () => {
    expect(defaultCustomId([stored()], "hc-light")).toBe("hc-light");
    expect(defaultCustomId([stored()], "theme_gone")).toBe("theme_1");
    expect(defaultCustomId([], null)).toBe("hc-dark");
  });

  it("copies colors from a starting point", () => {
    const themes = [stored()];
    expect(sourceColors("light", themes).colors).toEqual(BASE_COLORS.light);
    expect(sourceColors("hc-light", themes).base).toBe("light");
    expect(sourceColors({ copyOf: "theme_1" }, themes).colors["--bg"]).toBe("#101010");
    const copy = sourceColors("dark", themes);
    copy.colors["--bg"] = "#000000";
    expect(BASE_COLORS.dark["--bg"]).not.toBe("#000000");
  });

  it("makes names unique like the backend", () => {
    expect(uniqueName("Mine", ["Other"])).toBe("Mine");
    expect(uniqueName("Mine", ["mine", "Mine (2)"])).toBe("Mine (3)");
  });
});
