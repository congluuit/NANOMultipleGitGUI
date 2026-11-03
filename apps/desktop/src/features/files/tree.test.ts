import { describe, expect, it } from "vitest";
import { ancestors, buildTree, isUnder, parentOf } from "./tree";

describe("buildTree", () => {
  it("nests files under their folders, sorted by name", () => {
    const tree = buildTree(
      ["src/b.ts", "README.md", "src/lib/a10.ts", "src/lib/a9.ts", "a.txt"],
      ["empty"],
    );
    expect(tree.get("")).toEqual({ folders: ["empty", "src"], files: ["a.txt", "README.md"] });
    expect(tree.get("src")).toEqual({ folders: ["src/lib"], files: ["src/b.ts"] });
    // Numbers sort naturally.
    expect(tree.get("src/lib")?.files).toEqual(["src/lib/a9.ts", "src/lib/a10.ts"]);
    expect(tree.get("empty")).toEqual({ folders: [], files: [] });
  });
});

describe("path helpers", () => {
  it("finds parents and ancestors", () => {
    expect(parentOf("a/b/c.ts")).toBe("a/b");
    expect(parentOf("c.ts")).toBe("");
    expect(ancestors("a/b/c.ts")).toEqual(["a", "a/b"]);
  });

  it("tells whether a path is inside a folder", () => {
    expect(isUnder("src/a.ts", "src")).toBe(true);
    expect(isUnder("src", "src")).toBe(true);
    expect(isUnder("srcx/a.ts", "src")).toBe(false);
  });
});
