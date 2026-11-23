import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { RepoSearchResult, SearchMatch, WorkspaceSearchSummary } from "@/ipc";
import { useReveal } from "@/features/editor/open";
import { useWorkspace } from "@/stores/workspace";
import { useWorktreeStore } from "@/stores/worktree";
import { SearchPanel } from "./SearchPanel";
import { useSearchState } from "./store";
import { resetWorkspaceSearch } from "./workspaceSearch";

type Channel = { onmessage: (m: RepoSearchResult) => void };
const channels = vi.hoisted(() => [] as Channel[]);
const finish = vi.hoisted(() => [] as ((s: WorkspaceSearchSummary) => void)[]);
const ipc = vi.hoisted(() => ({
  workspaceSearch: vi.fn(
    () => new Promise<WorkspaceSearchSummary>((resolve) => finish.push(resolve)),
  ),
  repoSearch: vi.fn(async () => ({ matches: [], truncated: false })),
  transferCancel: vi.fn(async () => true),
  workspaceSetActive: vi.fn(async () => null),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
  Channel: class {
    onmessage: (m: RepoSearchResult) => void = () => {};
    constructor() {
      channels.push(this);
    }
  },
}));

/** The `i`-th item, failing the test when it is missing. */
function at<T>(list: readonly T[], i: number): T {
  const v = list[i];
  if (v === undefined) throw new Error(`no item ${i}`);
  return v;
}

const match = (path: string, line: number, text: string): SearchMatch => ({
  path,
  line,
  column: text.indexOf("needle") + 1,
  text,
  textStart: 0,
});

function renderPanel() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <SearchPanel repoId={1} />
    </QueryClientProvider>,
  );
}

describe("SearchPanel, all open repositories", () => {
  beforeEach(() => {
    initI18n("en");
    resetWorkspaceSearch();
    channels.length = 0;
    finish.length = 0;
    Object.values(ipc).forEach((f) => f.mockClear());
    useWorkspace.setState({
      tabs: [
        { id: 1, name: "alpha", path: "/r/alpha", missing: false },
        { id: 2, name: "beta", path: "/r/beta", missing: false },
        { id: 3, name: "gamma", path: "/r/gamma", missing: false },
      ],
      activeId: 1,
      loaded: true,
    });
    useSearchState.setState({ query: "needle", scope: "all" });
    useReveal.setState({ reveal: null });
  });

  it("groups streamed results by repository and opens a line in its repository", async () => {
    const user = userEvent.setup();
    renderPanel();
    expect(ipc.workspaceSearch).toHaveBeenCalledTimes(1);
    expect(ipc.repoSearch).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("0 of 3 repositories done");

    act(() => {
      at(channels, 0).onmessage({
        repoId: 3,
        name: "gamma",
        matches: [],
        truncated: false,
        missing: false,
        error: "unable to parse the pattern",
      });
      at(channels, 0).onmessage({
        repoId: 2,
        name: "beta",
        matches: [
          match("src/a.ts", 3, "const needle = 1;"),
          match("src/a.ts", 7, "needle()"),
          match("b.md", 1, "a needle"),
        ],
        truncated: true,
        missing: false,
        error: null,
      });
      at(channels, 0).onmessage({
        repoId: 1,
        name: "alpha",
        matches: [],
        truncated: false,
        missing: false,
        error: null,
      });
    });
    expect(screen.getByRole("status")).toHaveTextContent("3 of 3 repositories done");

    const list = screen.getByRole("list", { name: "Search results" });
    const repos = within(list)
      .getAllByRole("button")
      .filter((b) => /^(alpha|beta|gamma)/.test(b.textContent ?? ""));
    // Tab order; alpha has no match and is left out.
    expect(repos.map((b) => b.textContent)).toEqual(["beta3+", "gammaunable to parse the pattern"]);
    expect(within(list).getByText("a.ts")).toBeInTheDocument();
    expect(within(list).getAllByText("needle")).toHaveLength(3);

    await act(async () =>
      at(finish, 0)({ searched: 3, total: 3, truncated: false, skipped: 0, cancelled: false }),
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "3 results in 2 file(s) of 1 repository(ies).",
    );

    await user.click(within(list).getByRole("button", { name: /7\s*needle\(\)/ }));
    expect(useWorkspace.getState().activeId).toBe(2);
    expect(ipc.workspaceSetActive).toHaveBeenCalledWith(2);
    expect(useReveal.getState().reveal).toEqual({
      repoId: 2,
      path: "src/a.ts",
      line: 7,
      column: 1,
    });
    expect(useWorktreeStore.getState().diff[2]).toMatchObject({ path: "src/a.ts" });

    // Collapsing the repository hides its files.
    await user.click(at(repos, 0));
    expect(within(list).queryByText("a.ts")).not.toBeInTheDocument();
  });

  it("stops on request and switches back to this repository", async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole("button", { name: "Stop searching" }));
    expect(ipc.transferCancel).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status")).toHaveTextContent("Stopped");

    await user.click(screen.getByRole("radio", { name: "This repo" }));
    expect(useSearchState.getState().scope).toBe("repo");
    expect(screen.getByRole("textbox", { name: "Search in the repository" })).toBeInTheDocument();
    expect(ipc.repoSearch).toHaveBeenCalledWith(1, "needle", expect.anything(), 2000);
  });
});
