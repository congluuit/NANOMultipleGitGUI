import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoSearchResult, RepoTab, SearchMatch, WorkspaceSearchSummary } from "@/ipc";
import {
  cancelWorkspaceSearch,
  groupWorkspaceResults,
  resetWorkspaceSearch,
  runWorkspaceSearch,
  useWorkspaceSearch,
  workspaceTotals,
} from "./workspaceSearch";

type Channel = { onmessage: (m: RepoSearchResult) => void };
const channels = vi.hoisted(() => [] as Channel[]);
const pending = vi.hoisted(
  () => [] as { opId: string; resolve: (s: WorkspaceSearchSummary) => void }[],
);
const ipc = vi.hoisted(() => ({
  workspaceSearch: vi.fn(
    (opId: string) =>
      new Promise<WorkspaceSearchSummary>((resolve) => pending.push({ opId, resolve })),
  ),
  transferCancel: vi.fn(async () => true),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  Channel: class {
    onmessage: (m: RepoSearchResult) => void = () => {};
    constructor() {
      channels.push(this);
    }
  },
}));

const options = { caseSensitive: false, wholeWord: false, regex: false, untracked: true };

/** The `i`-th item, failing the test when it is missing. */
function at<T>(list: readonly T[], i: number): T {
  const v = list[i];
  if (v === undefined) throw new Error(`no item ${i}`);
  return v;
}

const match = (path: string, line: number): SearchMatch => ({
  path,
  line,
  column: 1,
  text: "needle",
  textStart: 0,
});

const result = (repoId: number, matches: SearchMatch[], extra = {}): RepoSearchResult => ({
  repoId,
  name: `repo${repoId}`,
  matches,
  truncated: false,
  missing: false,
  error: null,
  ...extra,
});

const tab = (id: number): RepoTab => ({ id, name: `repo${id}`, path: `/r/${id}`, missing: false });

const summary = (extra: Partial<WorkspaceSearchSummary> = {}): WorkspaceSearchSummary => ({
  searched: 2,
  total: 3,
  truncated: false,
  skipped: 0,
  cancelled: false,
  ...extra,
});

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("groupWorkspaceResults", () => {
  it("lists repositories in tab order with their files, and failures", () => {
    const results = [
      result(3, [match("b.txt", 4), match("a.txt", 1), match("b.txt", 9)]),
      result(1, []),
      result(2, [], { error: "bad pattern" }),
      result(4, [], { missing: true }),
      // A tab closed since the search started.
      result(9, [match("x", 1)]),
    ];
    const groups = groupWorkspaceResults(results, [tab(4), tab(1), tab(2), tab(3)]);
    expect(groups.map((g) => g.repoId)).toEqual([4, 2, 3]);
    expect(at(groups, 2).files).toEqual([
      ["b.txt", [match("b.txt", 4), match("b.txt", 9)]],
      ["a.txt", [match("a.txt", 1)]],
    ]);
    expect(at(groups, 1).error).toBe("bad pattern");
    expect(workspaceTotals(groups)).toEqual({ matches: 3, files: 2, repos: 1 });
  });
});

describe("runWorkspaceSearch", () => {
  beforeEach(() => {
    resetWorkspaceSearch();
    channels.length = 0;
    pending.length = 0;
    ipc.workspaceSearch.mockClear();
    ipc.transferCancel.mockClear();
  });

  it("streams each repository's result as it arrives", async () => {
    runWorkspaceSearch("needle", options, 3);
    expect(ipc.workspaceSearch).toHaveBeenCalledWith(
      expect.stringMatching(/^workspace-search-/),
      "needle",
      options,
      2000,
      at(channels, 0),
    );
    expect(useWorkspaceSearch.getState()).toMatchObject({ status: "running", repoCount: 3 });
    at(channels, 0).onmessage(result(2, [match("a", 1)]));
    expect(useWorkspaceSearch.getState().results.map((r) => r.repoId)).toEqual([2]);
    at(channels, 0).onmessage(result(1, [match("b", 1), match("b", 2)]));
    expect(useWorkspaceSearch.getState().results.map((r) => r.repoId)).toEqual([2, 1]);
    at(pending, 0).resolve(summary());
    await flush();
    expect(useWorkspaceSearch.getState()).toMatchObject({ status: "done", summary: summary() });

    // The same query and options again: nothing new runs, unless forced.
    runWorkspaceSearch("needle", options, 3);
    expect(ipc.workspaceSearch).toHaveBeenCalledTimes(1);
    runWorkspaceSearch("needle", options, 3, { force: true });
    expect(ipc.workspaceSearch).toHaveBeenCalledTimes(2);
    expect(useWorkspaceSearch.getState().results).toEqual([]);
  });

  it("a new query cancels the running search and ignores what it still sends", async () => {
    runWorkspaceSearch("old", options, 2);
    const oldOp = at(pending, 0).opId;
    at(channels, 0).onmessage(result(1, [match("a", 1)]));
    runWorkspaceSearch("new", options, 2);
    expect(ipc.transferCancel).toHaveBeenCalledWith(oldOp);
    expect(at(pending, 1).opId).not.toBe(oldOp);
    expect(useWorkspaceSearch.getState()).toMatchObject({ query: "new", results: [] });

    // Late messages and the answer of the old search change nothing.
    at(channels, 0).onmessage(result(2, [match("late", 1)]));
    at(pending, 0).resolve(summary({ cancelled: true }));
    await flush();
    expect(useWorkspaceSearch.getState()).toMatchObject({ status: "running", results: [] });

    at(channels, 1).onmessage(result(2, [match("b", 1)]));
    at(pending, 1).resolve(summary({ searched: 2, total: 1 }));
    await flush();
    expect(useWorkspaceSearch.getState().status).toBe("done");
    expect(useWorkspaceSearch.getState().results).toHaveLength(1);

    // An empty query clears everything without calling the backend.
    runWorkspaceSearch("  ", options, 2);
    expect(ipc.workspaceSearch).toHaveBeenCalledTimes(2);
    expect(useWorkspaceSearch.getState().status).toBe("idle");
  });

  it("changing an option starts a new search", () => {
    runWorkspaceSearch("x", options, 1);
    runWorkspaceSearch("x", { ...options, regex: true }, 1);
    expect(ipc.workspaceSearch).toHaveBeenCalledTimes(2);
    expect(ipc.transferCancel).toHaveBeenCalledTimes(1);
  });

  it("stopping keeps what was found and ignores the rest", async () => {
    runWorkspaceSearch("x", options, 3);
    at(channels, 0).onmessage(result(1, [match("a", 1)]));
    cancelWorkspaceSearch();
    expect(ipc.transferCancel).toHaveBeenCalledWith(at(pending, 0).opId);
    expect(useWorkspaceSearch.getState().status).toBe("cancelled");
    at(channels, 0).onmessage(result(2, [match("b", 1)]));
    at(pending, 0).resolve(summary({ cancelled: true, searched: 1 }));
    await flush();
    const s = useWorkspaceSearch.getState();
    expect(s.status).toBe("cancelled");
    expect(s.results.map((r) => r.repoId)).toEqual([1]);

    // Nothing running: stopping again does nothing.
    cancelWorkspaceSearch();
    expect(ipc.transferCancel).toHaveBeenCalledTimes(1);
  });

  it("shows an error of the command", async () => {
    ipc.workspaceSearch.mockImplementationOnce(async () => {
      throw { kind: "invalidInput", message: "operation is already running" };
    });
    runWorkspaceSearch("x", options, 1);
    await flush();
    expect(useWorkspaceSearch.getState()).toMatchObject({
      status: "error",
      error: "operation is already running",
    });
  });
});
