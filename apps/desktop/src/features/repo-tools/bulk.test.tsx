import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import {
  commands,
  type GuardFinding,
  type GuardReport,
  type RepoOverview,
  type RepoRefs,
  type WorkingTreeStatus,
} from "@/ipc";
import { accountKeys } from "@/features/accounts/queries";
import { OSS, WORK, makeContext, makeRemote } from "@/features/accounts/switcher/testData";
import { MISMATCH, commitReport, pushReport } from "@/features/guards/testData";
import { useTransferQueue } from "@/features/transfers/store";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import {
  canPushRow,
  checkState,
  filterCounts,
  folderLabel,
  groupRows,
  matchesFilter,
  moveItem,
  parentFolder,
  setMany,
  syncOrder,
  targetRows,
  type AccountTag,
} from "./bulk";
import { bulkCommit, bulkPush, useBulkRun } from "./bulkRun";
import { OverviewDialog } from "./OverviewDialog";
import { toolKeys } from "./queries";

const repo = (over: Partial<RepoOverview>): RepoOverview => ({
  repoId: 1,
  name: "api",
  path: "D:/Work/api",
  missing: false,
  branch: "main",
  upstream: "origin/main",
  ahead: 0,
  behind: 0,
  changes: 0,
  conflicts: 0,
  stashes: 0,
  operation: null,
  error: null,
  ...over,
});

const refsOf = (ahead: number, behind = 0): RepoRefs => ({
  head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
  local: [
    {
      name: "main",
      oid: "a".repeat(40),
      upstream: "origin/main",
      ahead,
      behind,
      upstreamGone: false,
      isHead: true,
    },
  ],
  remotes: [{ name: "origin", url: "https://github.com/acme/api.git", branches: [] }],
  tags: [],
  stashes: [],
});

const status = (staged: string[], conflicted: string[] = []): WorkingTreeStatus => {
  const entry = (path: string) => ({
    path,
    oldPath: null,
    kind: "modified" as const,
    conflict: null,
    submodule: false,
    size: 10,
  });
  return {
    branch: "main",
    headOid: "a".repeat(40),
    unborn: false,
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    staged: staged.map(entry),
    unstaged: [],
    conflicted: conflicted.map(entry),
  };
};

const noLargeFiles: GuardReport = {
  thresholds: { warnBytes: 1, blockBytes: null },
  findings: [],
  suggestCount: 0,
  warnCount: 0,
  blockCount: 0,
};

beforeEach(async () => {
  await initI18n("en");
  queryClient.clear();
  useUi.setState({ notice: null });
  useTransferQueue.setState({ items: {}, order: [] });
  useBulkRun.setState({ run: null });
  vi.restoreAllMocks();
});

describe("filters", () => {
  const rows = [
    repo({ repoId: 1, name: "clean" }),
    repo({ repoId: 2, name: "dirty", changes: 2 }),
    repo({ repoId: 3, name: "ahead", ahead: 1 }),
    repo({ repoId: 4, name: "both", behind: 3, conflicts: 1 }),
  ];

  it("counts and matches each filter", () => {
    expect(filterCounts(rows)).toEqual({ all: 4, changes: 2, outOfSync: 2 });
    expect(rows.filter((r) => matchesFilter(r, "changes")).map((r) => r.name)).toEqual([
      "dirty",
      "both",
    ]);
    expect(rows.filter((r) => matchesFilter(r, "outOfSync")).map((r) => r.name)).toEqual([
      "ahead",
      "both",
    ]);
    expect(rows.filter((r) => matchesFilter(r, "all"))).toHaveLength(4);
  });
});

describe("grouping", () => {
  it("reads the parent folder of Windows and POSIX paths", () => {
    expect(parentFolder("D:\\Work\\libs\\core")).toBe("D:/Work/libs");
    expect(parentFolder("D:/Work/api/")).toBe("D:/Work");
    expect(parentFolder("D:/api")).toBe("D:/");
    expect(parentFolder("/api")).toBe("/");
    expect(folderLabel("D:/Work/libs")).toBe("libs");
    expect(folderLabel("D:/")).toBe("D:/");
  });

  it("groups by folder, sorted by name, keeping the row order inside", () => {
    const rows = [
      repo({ repoId: 1, name: "web", path: "D:/Work/apps/web" }),
      repo({ repoId: 2, name: "core", path: "D:/Work/libs/core" }),
      repo({ repoId: 3, name: "admin", path: "D:\\Work\\apps\\admin" }),
    ];
    const groups = groupRows(rows, "folder", () => null);
    expect(groups.map((g) => g.label)).toEqual(["apps", "libs"]);
    expect(groups[0]!.rows.map((r) => r.name)).toEqual(["web", "admin"]);
    expect(groups[0]!.title).toBe("D:/Work/apps");
  });

  it("shows the full path when two folders have the same name", () => {
    const rows = [
      repo({ repoId: 1, path: "D:/A/src/one" }),
      repo({ repoId: 2, path: "E:/B/src/two" }),
    ];
    expect(groupRows(rows, "folder", () => null).map((g) => g.label)).toEqual([
      "D:/A/src",
      "E:/B/src",
    ]);
  });

  it("groups by account with unbound repositories last", () => {
    const tags: Record<number, AccountTag> = {
      1: { id: "w", label: "Work", color: "#111111" },
      3: { id: "p", label: "Personal", color: "#222222" },
    };
    const rows = [
      repo({ repoId: 1, name: "a" }),
      repo({ repoId: 2, name: "b" }),
      repo({ repoId: 3, name: "c" }),
    ];
    const groups = groupRows(rows, "account", (id) => tags[id] ?? null, "No account");
    expect(groups.map((g) => g.label)).toEqual(["Personal", "Work", "No account"]);
    expect(groups[1]!.color).toBe("#111111");
  });

  it("puts everything in one group without grouping", () => {
    expect(groupRows([repo({})], "none", () => null)).toHaveLength(1);
    expect(groupRows([], "none", () => null)).toHaveLength(0);
  });
});

describe("selection and order", () => {
  it("tells all, some or none", () => {
    expect(checkState([1, 2], new Set([1, 2]))).toBe("all");
    expect(checkState([1, 2], new Set([2]))).toBe("some");
    expect(checkState([1, 2], new Set())).toBe("none");
    expect(checkState([], new Set([1]))).toBe("none");
    expect([...setMany(new Set([1]), [2, 3], true)]).toEqual([1, 2, 3]);
    expect([...setMany(new Set([1, 2]), [1], false)]).toEqual([2]);
  });

  it("works on the selection, or on every visible row when nothing is selected", () => {
    const all = [repo({ repoId: 1 }), repo({ repoId: 2 }), repo({ repoId: 3 })];
    const visible = all.slice(1);
    expect(targetRows(all, visible, new Set([3, 1])).map((r) => r.repoId)).toEqual([1, 3]);
    expect(targetRows(all, visible, new Set()).map((r) => r.repoId)).toEqual([2, 3]);
    // A selected repository that was closed meanwhile does not count.
    expect(targetRows(all, visible, new Set([9])).map((r) => r.repoId)).toEqual([2, 3]);
  });

  it("keeps the user's order and moves items", () => {
    expect(syncOrder([3, 1], [1, 2, 3])).toEqual([3, 1, 2]);
    expect(syncOrder([3, 1, 2], [1, 2])).toEqual([1, 2]);
    expect(moveItem([1, 2, 3], 2, 0)).toEqual([3, 1, 2]);
    expect(moveItem([1, 2, 3], 0, 9)).toEqual([2, 3, 1]);
    expect(moveItem([1, 2, 3], 5, 0)).toEqual([1, 2, 3]);
  });

  it("pushes only branches ahead of an upstream", () => {
    expect(canPushRow(repo({ ahead: 1 }))).toBe(true);
    expect(canPushRow(repo({ ahead: 0 }))).toBe(false);
    expect(canPushRow(repo({ ahead: 1, upstream: null }))).toBe(false);
    expect(canPushRow(repo({ ahead: 1, branch: null }))).toBe(false);
    expect(canPushRow(repo({ ahead: 1, missing: true }))).toBe(false);
  });
});

function mockPushSide(opts: { guardFor?: number[]; failFor?: number[]; behindFor?: number[] }) {
  vi.spyOn(commands, "repoRefs").mockImplementation(async (id) =>
    refsOf(2, opts.behindFor?.includes(id) ? 1 : 0),
  );
  vi.spyOn(commands, "repoAccountContext").mockImplementation(async (id) =>
    makeContext({ repoId: id }),
  );
  vi.spyOn(commands, "lfsStatus").mockRejectedValue(new Error("no lfs"));
  const guard = vi.spyOn(commands, "pushGuard").mockImplementation(async (id) => ({
    status: "done",
    value: opts.guardFor?.includes(id) ? pushReport({ repoId: id }) : pushReport({ findings: [] }),
  }));
  const push = vi
    .spyOn(commands, "remotePush")
    .mockImplementation(async (id) =>
      opts.failFor?.includes(id)
        ? { status: "rejected", reason: "authFailed", message: "denied" }
        : { status: "done", value: { upToDate: false } },
    );
  return { guard, push };
}

const statusOf = (id: number) => useBulkRun.getState().run?.items.find((i) => i.repoId === id);

describe("bulk push", () => {
  const rows = [
    repo({ repoId: 1, name: "lib", ahead: 2 }),
    repo({ repoId: 2, name: "app", ahead: 2 }),
    repo({ repoId: 3, name: "web", ahead: 2 }),
  ];

  it("pushes one at a time in the given order", async () => {
    const { push, guard } = mockPushSide({});
    const order: number[] = [];
    push.mockImplementation(async (id) => {
      order.push(id);
      return { status: "done", value: { upToDate: false } };
    });
    const summary = await bulkPush([rows[2]!, rows[0]!, rows[1]!], true);
    expect(order).toEqual([3, 1, 2]);
    expect(guard).toHaveBeenCalledTimes(3);
    expect(summary?.done).toBe(3);
    expect(push.mock.calls[0]![3]).toBeNull();
    expect(useUi.getState().notice?.message).toBe("Pushed 3 repositories.");
  });

  it("never pushes a repository whose Push Guard has something to say", async () => {
    const { push } = mockPushSide({ guardFor: [2] });
    await bulkPush(rows, false);
    expect(push.mock.calls.map((c) => c[0])).toEqual([1, 3]);
    expect(statusOf(2)?.status).toBe("attention");
    expect(statusOf(2)?.reason).toContain("authored as hoang@acme.dev");
    expect(useUi.getState().notice?.hint).toContain("1 needs attention.");
  });

  it("stops after the first failure when asked to", async () => {
    const { push } = mockPushSide({ failFor: [1] });
    await bulkPush(rows, true);
    expect(push).toHaveBeenCalledTimes(1);
    expect(statusOf(1)?.status).toBe("failed");
    expect(statusOf(2)?.status).toBe("notRun");
    expect(statusOf(3)?.status).toBe("notRun");
  });

  it("continues after a failure when asked to", async () => {
    const { push } = mockPushSide({ failFor: [1] });
    await bulkPush(rows, false);
    expect(push).toHaveBeenCalledTimes(3);
    expect(useUi.getState().notice?.kind).toBe("error");
  });

  it("asks to pull first and checks the account of the remote", async () => {
    const { push } = mockPushSide({ behindFor: [1] });
    vi.spyOn(commands, "repoAccountContext").mockImplementation(async (id) =>
      id === 2
        ? makeContext({
            repoId: id,
            perRemote: [
              makeRemote({ remote: "origin", account: OSS }),
              makeRemote({ remote: "work", account: WORK }),
            ],
          })
        : makeContext({ repoId: id }),
    );
    await bulkPush(rows, false);
    expect(statusOf(1)?.reason).toContain("Pull first");
    expect(statusOf(2)?.status).toBe("attention");
    expect(statusOf(2)?.reason).toContain("another account");
    expect(push.mock.calls.map((c) => c[0])).toEqual([3]);
  });

  it("stops after the current repository when cancelled", async () => {
    const { push } = mockPushSide({});
    push.mockImplementation(async () => {
      useBulkRun.getState().cancel();
      return { status: "done", value: { upToDate: false } };
    });
    await bulkPush(rows, false);
    expect(push).toHaveBeenCalledTimes(1);
    expect(statusOf(1)?.status).toBe("done");
    expect(statusOf(2)?.status).toBe("notRun");
  });
});

describe("quick commit", () => {
  const rows = [
    repo({ repoId: 1, name: "a", changes: 1 }),
    repo({ repoId: 2, name: "b", changes: 1 }),
    repo({ repoId: 3, name: "c", changes: 1 }),
    repo({ repoId: 4, name: "d", changes: 1 }),
  ];

  function mockCommitSide() {
    vi.spyOn(commands, "repoRefs").mockResolvedValue(refsOf(0));
    vi.spyOn(commands, "worktreeStatus").mockImplementation(async (id) =>
      id === 4 ? status([]) : status([`file${id}.txt`]),
    );
    vi.spyOn(commands, "commitGuard").mockImplementation(async (id) =>
      id === 2 ? MISMATCH : commitReport({ repoId: id }),
    );
    vi.spyOn(commands, "largeFileCheck").mockImplementation(async (id) =>
      id === 3
        ? {
            ...noLargeFiles,
            findings: [{ path: "big.bin", level: "warn" } as GuardFinding],
          }
        : noLargeFiles,
    );
    return vi
      .spyOn(commands, "commitCreate")
      .mockResolvedValue({ hash: "abcdef1234", summary: "Bump" });
  }

  it("commits only staged changes of repositories whose guards are quiet", async () => {
    const create = mockCommitSide();
    const stageFiles = vi.spyOn(commands, "stageFiles");
    const stageAll = vi.spyOn(commands, "stageAll");
    await bulkCommit(rows, "Bump version");
    expect(create.mock.calls.map((c) => c[0])).toEqual([1]);
    expect(create.mock.calls[0]![1]).toBe("Bump version");
    expect(create.mock.calls[0]![2]).toEqual({ amend: false, signoff: false, allowEmpty: false });
    expect(create.mock.calls[0]![3]).toBeNull();
    expect(stageFiles).not.toHaveBeenCalled();
    expect(stageAll).not.toHaveBeenCalled();
    expect(statusOf(2)?.status).toBe("attention");
    expect(statusOf(2)?.reason).toContain("does not belong");
    expect(statusOf(3)?.status).toBe("attention");
    expect(statusOf(3)?.reason).toContain("LFS");
    expect(statusOf(4)?.status).toBe("skipped");
    expect(useUi.getState().notice?.message).toBe("Committed in 1 repository.");
  });
});

describe("OverviewDialog", () => {
  const rows = [
    repo({ repoId: 1, name: "core", path: "D:/Work/libs/core", ahead: 2 }),
    repo({ repoId: 2, name: "web", path: "D:/Work/apps/web", changes: 3 }),
    repo({ repoId: 3, name: "admin", path: "D:/Work/apps/admin", ahead: 1, behind: 1 }),
  ];

  function setup() {
    queryClient.setQueryData(toolKeys.overview(), rows);
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <OverviewDialog onClose={() => {}} />
      </QueryClientProvider>,
    );
    return user;
  }

  it("filters with counts and groups by folder", async () => {
    const user = setup();
    expect(screen.getByRole("radio", { name: "All (3)" })).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Has changes (1)" }));
    expect(screen.queryByText("core")).not.toBeInTheDocument();
    expect(screen.getByText("web")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Out of sync (2)" }));
    expect(screen.getByText("core")).toBeInTheDocument();
    expect(screen.queryByText("web")).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "All (3)" }));
    await user.click(screen.getByRole("radio", { name: "By folder" }));
    const apps = screen.getByRole("button", { name: /apps/ });
    expect(apps).toHaveAttribute("aria-expanded", "true");
    await user.click(apps);
    expect(screen.queryByText("web")).not.toBeInTheDocument();
    expect(screen.getByText("core")).toBeInTheDocument();
  });

  it("groups by account", async () => {
    queryClient.setQueryData(accountKeys.repoContext(1), makeContext({ repoId: 1 }));
    const user = setup();
    await user.click(screen.getByRole("radio", { name: "By account" }));
    expect(screen.getByRole("button", { name: /Công ty/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /No account/ })).toBeInTheDocument();
  });

  it("selects repositories and acts on the selection", async () => {
    const user = setup();
    expect(screen.getByRole("button", { name: "Fetch all" })).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "Select web" }));
    expect(screen.getByText("1 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Fetch selected (1)" })).toBeInTheDocument();
    // Nothing to push in "web".
    expect(screen.getByRole("button", { name: /Push/ })).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: "Select every repository shown" }));
    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Push/ })).toHaveTextContent("Push… (2)");
  });

  it("orders the push and runs it", async () => {
    const { push } = mockPushSide({});
    const user = setup();
    await user.click(screen.getByRole("button", { name: /Push/ }));
    const list = screen.getByRole("list", { name: "Push order" });
    const names = () =>
      within(list)
        .getAllByRole("listitem")
        .map((li) => li.textContent);
    expect(names()[0]).toContain("core");
    await user.click(screen.getByRole("button", { name: "Move admin up" }));
    expect(names()[0]).toContain("admin");
    await user.click(screen.getByRole("radio", { name: "Continue with the rest" }));
    await user.click(screen.getByRole("button", { name: "Push 2 repositories" }));
    await screen.findByText("2 of 2 finished", undefined, { timeout: 5000 });
    expect(push.mock.calls.map((c) => c[0])).toEqual([3, 1]);
    expect(screen.getAllByText("Done")).toHaveLength(2);
  });

  it("shows which repositories will be committed before committing", async () => {
    vi.spyOn(commands, "worktreeStatus").mockImplementation(async (id) =>
      id === 2 ? status(["a.txt", "b.txt"]) : status([]),
    );
    const user = setup();
    await user.click(screen.getByRole("button", { name: /Commit/ }));
    expect(await screen.findByText("2 staged files")).toBeInTheDocument();
    expect(screen.getByText("Will be committed (1)")).toBeInTheDocument();
    expect(screen.getByText("Left alone (2)")).toBeInTheDocument();
    const start = screen.getByRole("button", { name: "Commit in 1 repository" });
    expect(start).toBeDisabled();
    await user.type(screen.getByRole("textbox", { name: "Commit message" }), "Bump");
    expect(start).toBeEnabled();
  });
});
