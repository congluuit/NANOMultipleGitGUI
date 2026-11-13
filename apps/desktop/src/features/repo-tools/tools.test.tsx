import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { commands, type BisectState, type ReflogEntry, type RepoOverview } from "@/ipc";
import { mergeRecent } from "@/features/branch-switch/recentBranches";
import { mergeLimits } from "@/features/guards/limits";
import { osMessage } from "@/features/notifications/osNotify";
import { useTransferQueue } from "@/features/transfers/store";
import { useUi } from "@/stores/ui";
import { patchFileName } from "./actions";
import { BisectBannerView } from "./Bisect";
import {
  barPercent,
  bisectStepsLeft,
  filterReflog,
  nestedFolders,
  parseFolderLines,
} from "./model";
import { canFetch, canPull, fetchAllRepos, overviewTotals, pullAllRepos } from "./overview";

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

const bisect = (over: Partial<BisectState>): BisectState => ({
  active: true,
  current: "1234567890abcdef1234567890abcdef12345678",
  currentSubject: "Refactor session storage",
  bad: "b".repeat(40),
  good: ["a".repeat(40)],
  remaining: 16,
  firstBad: null,
  firstBadSubject: null,
  start: "main",
  ...over,
});

beforeEach(async () => {
  await initI18n("en");
  useUi.setState({ notice: null });
  useTransferQueue.setState({ items: {}, order: [] });
  vi.restoreAllMocks();
});

describe("all repositories", () => {
  const rows = [
    repo({ repoId: 1, name: "behind", behind: 2 }),
    repo({ repoId: 2, name: "dirty", behind: 4, changes: 3 }),
    repo({ repoId: 3, name: "rebasing", behind: 1, operation: "rebase" }),
    repo({ repoId: 4, name: "local", upstream: null, ahead: 0 }),
    repo({ repoId: 5, name: "gone", missing: true }),
    repo({ repoId: 6, name: "ahead", ahead: 2 }),
  ];

  it("pulls only repositories that are behind and clean", () => {
    expect(rows.filter(canPull).map((r) => r.name)).toEqual(["behind"]);
    expect(rows.filter(canFetch).map((r) => r.name)).not.toContain("gone");
    expect(overviewTotals(rows)).toEqual({ behind: 3, ahead: 1, dirty: 1, pullable: 1 });
  });

  it("fetches every repository and sums the round up in one message", async () => {
    const fetch = vi
      .spyOn(commands, "remoteFetch")
      .mockImplementation(async (repoId) =>
        repoId === 2
          ? { status: "rejected", reason: "authFailed", message: "denied" }
          : { status: "done", value: { updatedRefs: 1 } },
      );
    vi.spyOn(commands, "workspaceOverview").mockResolvedValue(rows);
    const result = await fetchAllRepos(rows);
    expect(fetch).toHaveBeenCalledTimes(5);
    expect(result.done).toHaveLength(4);
    expect(result.failed.map((f) => f.name)).toEqual(["dirty"]);
    const notice = useUi.getState().notice!;
    expect(notice.kind).toBe("error");
    expect(notice.message).toBe("Fetched 4 repositories, 1 failed.");
    expect(notice.detail).toContain("dirty:");
  });

  it("pulls fast-forward only", async () => {
    const pull = vi.spyOn(commands, "remotePull").mockResolvedValue({
      status: "done",
      value: { result: "fastForward", conflicts: [], pending: null, head: null },
    });
    vi.spyOn(commands, "workspaceOverview").mockResolvedValue(rows);
    await pullAllRepos(rows);
    expect(pull).toHaveBeenCalledTimes(1);
    expect(pull.mock.calls[0]![0]).toBe(1);
    expect(pull.mock.calls[0]![4]).toBe("fastForwardOnly");
    expect(useUi.getState().notice?.message).toBe("Pulled 1 repository.");
  });
});

describe("tool helpers", () => {
  it("filters the reflog by action, message, subject or id", () => {
    const entries: ReflogEntry[] = [
      {
        index: 0,
        oid: "aaa111",
        action: "checkout",
        message: "moving from a to b",
        subject: "S1",
        time: 1,
      },
      {
        index: 1,
        oid: "bbb222",
        action: "commit",
        message: "Fix login",
        subject: "Fix login",
        time: 2,
      },
    ];
    expect(filterReflog(entries, "LOGIN").map((e) => e.index)).toEqual([1]);
    expect(filterReflog(entries, "aaa").map((e) => e.index)).toEqual([0]);
    expect(filterReflog(entries, " ")).toHaveLength(2);
  });

  it("reads folders typed one per line", () => {
    expect(parseFolderLines(" src\\app \n\n/docs/\nsrc/app\n")).toEqual(["src/app", "docs"]);
    expect(nestedFolders({ folders: ["src", "packages/ui"], available: ["src", "docs"] })).toEqual([
      "packages/ui",
    ]);
  });

  it("sizes bars and counts the tests a bisect still needs", () => {
    expect(barPercent(0, 10)).toBe(0);
    expect(barPercent(1, 1000)).toBe(2);
    expect(barPercent(5, 10)).toBe(50);
    expect(bisectStepsLeft({ remaining: 16, firstBad: null })).toBe(4);
    expect(bisectStepsLeft({ remaining: 1, firstBad: null })).toBe(0);
    expect(bisectStepsLeft({ remaining: 9, firstBad: "abc" })).toBe(0);
  });

  it("names patch files after the commit subject", () => {
    expect(patchFileName("Sửa lỗi đăng nhập (#12)", "abc1234")).toBe("Sua-loi-dang-nhap-12.patch");
    expect(patchFileName("???", "abc1234")).toBe("abc1234.patch");
  });

  it("merges recent branches and server limits", () => {
    expect(mergeRecent(["a", "b"], ["b", "c"])).toEqual(["a", "b", "c"]);
    expect(mergeRecent(["a"], undefined)).toEqual(["a"]);
    expect(
      mergeLimits({ "github.com": 100, "x.io": 50 }, { "github.com": 80, "y.io": 10 }),
    ).toEqual({
      "github.com": 80,
      "x.io": 50,
      "y.io": 10,
    });
  });

  it("builds the system notification of a notice", () => {
    expect(osMessage({ kind: "info", message: "Pushed main." }, "api")).toEqual({
      title: "api",
      body: "Pushed main.",
    });
    expect(osMessage({ kind: "error", message: "Push failed", hint: "Pull first" }, null)).toEqual({
      title: "Push failed",
      body: "Pull first",
    });
  });
});

describe("bisect banner", () => {
  it("asks whether the tested commit works and sends the answer", async () => {
    const mark = vi
      .spyOn(commands, "bisectMark")
      .mockResolvedValue({ status: "done", value: bisect({ remaining: 8 }) });
    render(<BisectBannerView repoId={7} state={bisect({})} />);
    expect(screen.getByText("Test 1234567")).toBeInTheDocument();
    expect(screen.getByText("about 4 more tests")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "It is broken" }));
    expect(mark).toHaveBeenCalledWith(7, "bad");
  });

  it("names the first broken commit when it is found", async () => {
    const reset = vi
      .spyOn(commands, "bisectReset")
      .mockResolvedValue({ status: "done", value: null });
    render(
      <BisectBannerView
        repoId={7}
        state={bisect({ firstBad: "f".repeat(40), firstBadSubject: "Cache tokens", remaining: 1 })}
      />,
    );
    expect(screen.getByText("First broken commit:")).toBeInTheDocument();
    expect(screen.getByText(/Cache tokens/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "It works" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));
    expect(reset).toHaveBeenCalledWith(7);
  });

  it("shows nothing without a bisect", () => {
    const { container } = render(<BisectBannerView repoId={7} state={bisect({ active: false })} />);
    expect(container).toBeEmptyDOMElement();
  });
});
