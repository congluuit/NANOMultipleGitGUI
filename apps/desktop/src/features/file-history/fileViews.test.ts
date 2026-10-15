import { beforeEach, describe, expect, it } from "vitest";
import { closeFileView, jumpToCommit, openFileView, useFileViewStore } from "@/stores/fileView";
import { useGraphStore } from "@/stores/graph";
import { useWorktreeStore } from "@/stores/worktree";
import { normalizeRepoPath } from "./commands";

describe("normalizeRepoPath", () => {
  it("accepts repository-relative paths only", () => {
    expect(normalizeRepoPath("  src/main.rs ")).toBe("src/main.rs");
    expect(normalizeRepoPath("./a\\b.txt")).toBe("a/b.txt");
    expect(normalizeRepoPath("/top.txt")).toBe("top.txt");
    for (const bad of ["", "   ", "../x", "a/../../b", "-p", "C:/x", "c:\\x"]) {
      expect(normalizeRepoPath(bad), bad).toBeNull();
    }
  });
});

describe("file view store", () => {
  beforeEach(() => {
    useFileViewStore.setState({ views: {} });
    useWorktreeStore.setState({ diff: {} });
  });

  it("opening a view closes the diff; opening a diff closes the view", () => {
    useWorktreeStore.getState().openDiff(1, { side: "unstaged", path: "a" });
    openFileView(1, { kind: "history", path: "a" });
    expect(useWorktreeStore.getState().diff[1]).toBeNull();
    expect(useFileViewStore.getState().views[1]).toEqual({ kind: "history", path: "a" });
    useWorktreeStore.getState().openDiff(1, { side: "staged", path: "b" });
    expect(useFileViewStore.getState().views[1]).toBeNull();
    // Other repositories keep their view.
    openFileView(2, { kind: "blame", path: "x", rev: null });
    useWorktreeStore.getState().openDiff(1, { side: "unstaged", path: "c" });
    expect(useFileViewStore.getState().views[2]).not.toBeNull();
    closeFileView(2);
    expect(useFileViewStore.getState().views[2]).toBeNull();
  });

  it("jumping to a commit shows the graph", () => {
    openFileView(1, { kind: "file", path: "a", commit: "abc1234" });
    jumpToCommit(1, "abc1234");
    expect(useFileViewStore.getState().views[1]).toBeNull();
    expect(useGraphStore.getState().jump).toMatchObject({ repoId: 1, oid: "abc1234" });
    // A jump requested elsewhere (sidebar) also closes the view.
    openFileView(1, { kind: "history", path: "a" });
    useGraphStore.getState().requestJump(1, "def5678");
    expect(useFileViewStore.getState().views[1]).toBeNull();
  });
});
