// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseTokensCss } from "@/theme/tokens";
import { terminalFont, withAlpha, xtermTheme } from "./theme";

const css = readFileSync(new URL("../../theme/tokens.css", import.meta.url), "utf8");
const tokens = parseTokensCss(css);

describe("xterm theme", () => {
  it("is built from the design tokens of each theme", () => {
    for (const mode of ["light", "dark"] as const) {
      const map = tokens[mode];
      const theme = xtermTheme(mode, (name) => map[name] ?? "");
      expect(theme.background).toBe(map["--surface"]);
      expect(theme.foreground).toBe(map["--text"]);
      expect(theme.cursor).toBe(map["--primary"]);
      expect(theme.red).toBe(map["--danger"]);
      expect(theme.green).toBe(map["--success"]);
      expect(theme.selectionBackground).toBe(`${map["--primary"]}4d`);
      // Every color is defined (no missing token).
      for (const [key, value] of Object.entries(theme)) expect(value, key).toMatch(/^#/);
    }
  });

  it("keeps ANSI white readable on the light background", () => {
    const light = tokens.light;
    const theme = xtermTheme("light", (name) => light[name] ?? "");
    expect(theme.white).not.toBe(light["--surface"]);
    expect(theme.black).toBe(light["--text"]);
  });

  it("adds alpha only to #rrggbb colors", () => {
    expect(withAlpha("#ff8a3d", 0.5)).toBe("#ff8a3d80");
    expect(withAlpha("#ff8a3d", 2)).toBe("#ff8a3dff");
    expect(withAlpha("rgb(1,2,3)", 0.5)).toBe("rgb(1,2,3)");
  });

  it("uses the --font-mono stack", () => {
    expect(terminalFont(() => '"Cascadia Code", monospace')).toBe('"Cascadia Code", monospace');
    expect(terminalFont(() => "")).toContain("monospace");
  });
});
