import { describe, expect, it } from "vitest";
import { parseFileQuery, preparePaths, rankPaths } from "./quickOpenModel";

const paths = preparePaths([
  "apps/desktop/src/features/shell/AppShell.tsx",
  "apps/desktop/src/features/shell/Sidebar.tsx",
  "apps/desktop/src/App.tsx",
  "docs/shell-notes.md",
  "crates/git-engine/src/edit.rs",
]);

describe("rankPaths", () => {
  it("prefers hits in the file name", () => {
    expect(rankPaths("app", paths, 3)[0]).toBe("apps/desktop/src/App.tsx");
    expect(rankPaths("appsh", paths, 5)[0]).toBe("apps/desktop/src/features/shell/AppShell.tsx");
    expect(rankPaths("shell", paths, 5)[0]).toBe("docs/shell-notes.md");
  });

  it("needs every token and accepts folder parts", () => {
    expect(rankPaths("engine edit", paths, 5)).toEqual(["crates/git-engine/src/edit.rs"]);
    expect(rankPaths("shell/side", paths, 5)).toEqual([
      "apps/desktop/src/features/shell/Sidebar.tsx",
    ]);
    expect(rankPaths("zzz", paths, 5)).toEqual([]);
  });

  it("keeps the order for an empty query", () => {
    expect(rankPaths("  ", paths, 2)).toEqual([
      "apps/desktop/src/features/shell/AppShell.tsx",
      "apps/desktop/src/features/shell/Sidebar.tsx",
    ]);
  });
});

describe("parseFileQuery", () => {
  it("splits a line and column", () => {
    expect(parseFileQuery("src/a.ts:12:4")).toEqual({ text: "src/a.ts", line: 12, column: 4 });
    expect(parseFileQuery("a.ts:7")).toEqual({ text: "a.ts", line: 7, column: null });
    expect(parseFileQuery(" src\\b.ts ")).toEqual({ text: "src/b.ts", line: null, column: null });
  });
});
