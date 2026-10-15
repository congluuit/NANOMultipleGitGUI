// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { CONTRAST_RULES, contrastRatio } from "./contrast";
import { COLOR_TOKENS, LANE_TOKENS, parseTokensCss } from "./tokens";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
const themes = parseTokensCss(css);

describe("contrastRatio", () => {
  it("matches known WCAG values", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(contrastRatio("#ffffff", "#ffffff")).toBeCloseTo(1, 5);
    expect(contrastRatio("#767676", "#ffffff")).toBeCloseTo(4.54, 2);
  });

  it("accepts the short #rgb form that release builds' minified CSS produces", () => {
    expect(contrastRatio("#000", "#fff")).toBeCloseTo(21, 5);
    expect(contrastRatio(" #FFF ", "#ffffff")).toBeCloseTo(1, 5);
  });
});

describe.each(Object.entries(themes))("%s theme", (_name, tokens) => {
  it("defines every color token in both themes", () => {
    for (const token of [...COLOR_TOKENS, ...LANE_TOKENS]) {
      expect(tokens[token], token).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it.each(CONTRAST_RULES.map((r) => [`${r.fg} on ${r.bg} ≥ ${r.min}`, r] as const))(
    "%s",
    (_label, rule) => {
      const ratio = contrastRatio(tokens[rule.fg]!, tokens[rule.bg]!);
      expect(ratio).toBeGreaterThanOrEqual(rule.min);
    },
  );
});
