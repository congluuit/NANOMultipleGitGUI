import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PERSONAL, seedAccounts, makeAccount } from "@/features/accounts/switcher/testData";
import { initI18n } from "@/i18n";
import {
  commands,
  type HostedRemote,
  type PullCreateContext,
  type PullDetail,
  type PullSummary,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useHostingNav } from "@/stores/hosting";
import { useUi } from "@/stores/ui";
import { CreatePullDialogHost } from "./CreatePullDialog";
import { PullDetailView } from "./PullDetailView";
import { PullSidebarList } from "./PullSidebarList";
import { usePullUi } from "./store";

const PERSONAL2 = makeAccount({ id: "acc-personal2", username: "alt", label: "Phụ", sortOrder: 3 });

const wrap = (ui: React.ReactNode) =>
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);

const remote = (over: Partial<HostedRemote> = {}): HostedRemote => ({
  remote: "origin",
  host: "github.com",
  fullName: "acme/app",
  accountId: PERSONAL.id,
  family: "github",
  hasToken: true,
  ...over,
});

const summary = (n: number, over: Partial<PullSummary> = {}): PullSummary => ({
  number: n,
  title: `PR ${n}`,
  state: "open",
  draft: false,
  author: { username: "alice", displayName: null, avatarUrl: null, webUrl: null },
  sourceBranch: "feat",
  targetBranch: "main",
  labels: [],
  comments: 0,
  createdAt: null,
  updatedAt: null,
  webUrl: `https://github.com/acme/app/pull/${n}`,
  ...over,
});

const detail = (over: Partial<PullDetail> = {}): PullDetail => ({
  number: 5,
  title: "<img src=x onerror=alert(1)> Fix",
  body: "<script>alert(1)</script>\nsee https://example.com",
  state: "open",
  draft: false,
  author: { username: "alice", displayName: null, avatarUrl: null, webUrl: null },
  sourceBranch: "feat",
  targetBranch: "main",
  sourceRepo: "acme/app",
  targetRepo: "acme/app",
  fromFork: false,
  headSha: "abc1234",
  baseSha: "def5678",
  startSha: null,
  mergeState: "clean",
  mergeStateRaw: "clean",
  mergeMethods: ["merge", "squash"],
  deleteSourceDefault: false,
  labels: ["bug"],
  assignees: [],
  comments: 0,
  reviewComments: 0,
  commits: 1,
  additions: 1,
  deletions: 0,
  changedFiles: 1,
  createdAt: null,
  updatedAt: null,
  mergedAt: null,
  closedAt: null,
  mergedBy: null,
  webUrl: "https://github.com/acme/app/pull/5",
  ...over,
});

beforeEach(() => {
  initI18n("vi");
  queryClient.clear();
  seedAccounts(queryClient, { accounts: [PERSONAL, PERSONAL2] });
  useHostingNav.setState({ pullRequest: null, issue: null });
  usePullUi.setState({ filters: {}, create: null });
  useUi.setState({ notice: null });
  vi.spyOn(commands, "pullReviews").mockResolvedValue({
    decision: "approved",
    reviewers: [
      {
        user: { username: "bob", displayName: null, avatarUrl: null, webUrl: null },
        state: "approved",
      },
    ],
    teams: [],
    approvalsLeft: null,
    approvedByMe: null,
    approvalsUnavailable: false,
  });
});

afterEach(() => vi.restoreAllMocks());

describe("PullSidebarList", () => {
  it("lists pull requests and opens one", async () => {
    vi.spyOn(commands, "hostedRemoteList").mockResolvedValue([remote()]);
    const list = vi.spyOn(commands, "pullList").mockResolvedValue({
      items: [summary(1), summary(2, { draft: true })],
      nextPage: null,
      total: null,
    });
    wrap(
      <ul>
        <PullSidebarList repoId={7} />
      </ul>,
    );
    const row = await screen.findByText("PR 1");
    expect(list).toHaveBeenCalledWith(
      PERSONAL.id,
      "acme/app",
      expect.objectContaining({ state: "open", page: 1 }),
    );
    expect(screen.getByText("Bản nháp")).toBeInTheDocument();
    await userEvent.click(row);
    expect(useHostingNav.getState().pullRequest).toEqual({
      accountId: PERSONAL.id,
      fullName: "acme/app",
      number: 1,
      repoId: 7,
    });
  });

  it("guides when the remote has no account or no token", async () => {
    vi.spyOn(commands, "hostedRemoteList").mockResolvedValue([remote({ accountId: null })]);
    const list = vi.spyOn(commands, "pullList");
    wrap(
      <ul>
        <PullSidebarList repoId={7} />
      </ul>,
    );
    expect(await screen.findByText(/chưa gắn tài khoản/)).toBeInTheDocument();
    expect(list).not.toHaveBeenCalled();
  });

  it("shows a fixable error instead of throwing", async () => {
    vi.spyOn(commands, "hostedRemoteList").mockResolvedValue([remote()]);
    vi.spyOn(commands, "pullList").mockRejectedValue({
      kind: "provider",
      message: "401",
      accountId: PERSONAL.id,
      error: { kind: "unauthorized", message: "Bad credentials" },
    });
    wrap(
      <ul>
        <PullSidebarList repoId={7} />
      </ul>,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(/từ chối token/);
    expect(screen.getByText(/Đăng nhập lại Cá nhân/)).toBeInTheDocument();
  });
});

describe("PullDetailView", () => {
  const ref = { accountId: PERSONAL.id, fullName: "acme/app", number: 5, repoId: 7 };

  beforeEach(() => {
    vi.spyOn(commands, "pullCommits").mockResolvedValue([]);
    vi.spyOn(commands, "pullComments").mockResolvedValue([]);
  });

  it("renders provider text as plain text", async () => {
    vi.spyOn(commands, "pullDetail").mockResolvedValue(detail());
    const { container } = wrap(<PullDetailView pullRef={ref} />);
    expect(await screen.findByText(/<script>alert\(1\)<\/script>/)).toBeInTheDocument();
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img[src='x']")).toBeNull();
    expect(screen.getByRole("link", { name: "https://example.com" })).toBeInTheDocument();
  });

  it("merges as the chosen account after confirming", async () => {
    vi.spyOn(commands, "pullDetail").mockResolvedValue(detail());
    const merge = vi
      .spyOn(commands, "pullMerge")
      .mockResolvedValue({ sha: "m", branchDeleted: false, deleteError: null });
    wrap(<PullDetailView pullRef={ref} />);
    const side = await screen.findByText("Cách merge");
    await userEvent.selectOptions(within(side.closest("label")!).getByRole("combobox"), "squash");
    // "with ● account" switch for this action only.
    const acting = screen.getAllByTestId("pull-acting-as")[0]!;
    await userEvent.click(acting);
    await userEvent.click(await screen.findByRole("menuitem", { name: /Phụ/ }));
    await userEvent.click(screen.getByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Merge" }));
    await waitFor(() =>
      expect(merge).toHaveBeenCalledWith(PERSONAL2.id, "acme/app", 5, {
        method: "squash",
        deleteSourceBranch: false,
        expectedHeadSha: "abc1234",
      }),
    );
  });

  it("disables merging a conflicting pull request and explains the fix", async () => {
    vi.spyOn(commands, "pullDetail").mockResolvedValue(detail({ mergeState: "conflicts" }));
    wrap(<PullDetailView pullRef={ref} />);
    expect(await screen.findByText(/Có xung đột với main/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Merge" })).toBeDisabled();
    expect(screen.getByText(/bấm Checkout/)).toBeInTheDocument();
  });

  it("shows the files tab with line threads", async () => {
    vi.spyOn(commands, "pullDetail").mockResolvedValue(detail());
    vi.spyOn(commands, "pullFiles").mockResolvedValue({
      truncated: false,
      files: [
        {
          path: "a.rs",
          oldPath: null,
          status: "modified",
          additions: 1,
          deletions: 1,
          patch: "@@ -1 +1 @@\n-old\n+new\n",
        },
      ],
    });
    vi.spyOn(commands, "pullComments").mockResolvedValue([
      {
        id: "line-1",
        kind: "line",
        author: { username: "bob", displayName: null, avatarUrl: null, webUrl: null },
        body: "why new?",
        createdAt: null,
        updatedAt: null,
        threadId: "1",
        anchor: { path: "a.rs", line: 1, side: "new", originalLine: 1, diffHunk: null },
        reviewState: null,
        resolved: null,
        webUrl: null,
      },
    ]);
    const post = vi.spyOn(commands, "pullCommentCreate").mockResolvedValue({
      id: "line-2",
      kind: "line",
      author: null,
      body: "ok",
      createdAt: null,
      updatedAt: null,
      threadId: "1",
      anchor: null,
      reviewState: null,
      resolved: null,
      webUrl: null,
    });
    wrap(<PullDetailView pullRef={ref} />);
    await userEvent.click(await screen.findByRole("tab", { name: /File thay đổi/ }));
    expect(await screen.findByText("why new?")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Trả lời" }));
    await userEvent.type(screen.getByPlaceholderText("Viết câu trả lời…"), "ok");
    await userEvent.click(screen.getAllByRole("button", { name: "Trả lời" }).at(-1)!);
    await waitFor(() =>
      expect(post).toHaveBeenCalledWith(
        PERSONAL.id,
        "acme/app",
        5,
        { kind: "reply", threadId: "1" },
        "ok",
      ),
    );
  });
});

describe("CreatePullDialog", () => {
  const ctx = (over: Partial<PullCreateContext> = {}): PullCreateContext => ({
    branch: "feat/x",
    remotes: [remote()],
    target: remote(),
    targetBranch: "main",
    push: { remote: "origin", remoteBranch: "feat/x", pushed: true, ahead: 0 },
    sourceRef: "feat/x",
    crossRepo: false,
    crossRepoUnsupported: false,
    suggestedTitle: "Add x",
    suggestedBody: "- one",
    commitCount: 1,
    ...over,
  });

  it("asks to push an unpushed branch first", async () => {
    vi.spyOn(commands, "pullCreateContext").mockResolvedValue(
      ctx({ push: { remote: "origin", remoteBranch: "feat/x", pushed: false, ahead: 0 } }),
    );
    usePullUi.setState({ create: { repoId: 7, branch: null } });
    wrap(<CreatePullDialogHost />);
    expect(await screen.findByTestId("pull-create-push")).toHaveTextContent(
      /chưa có trên remote origin/,
    );
    expect(screen.getByRole("button", { name: "Tạo PR" })).toBeDisabled();
  });

  it("creates with the suggested title, reviewers and labels, then opens it", async () => {
    vi.spyOn(commands, "pullCreateContext").mockResolvedValue(ctx());
    const create = vi
      .spyOn(commands, "pullCreate")
      .mockResolvedValue({ number: 9, webUrl: "https://github.com/acme/app/pull/9", warnings: [] });
    usePullUi.setState({ create: { repoId: 7, branch: null } });
    wrap(<CreatePullDialogHost />);
    expect(await screen.findByDisplayValue("Add x")).toBeInTheDocument();
    await userEvent.type(screen.getByPlaceholderText(/tên đăng nhập/), "@bob, carol");
    await userEvent.type(screen.getByPlaceholderText("bug, ui"), "bug");
    await userEvent.click(screen.getByLabelText("Tạo dưới dạng bản nháp"));
    await userEvent.click(screen.getByRole("button", { name: "Tạo PR" }));
    await waitFor(() =>
      expect(create).toHaveBeenCalledWith(PERSONAL.id, "acme/app", {
        sourceBranch: "feat/x",
        targetBranch: "main",
        title: "Add x",
        body: "- one",
        draft: true,
        reviewers: ["bob", "carol"],
        labels: ["bug"],
        removeSourceBranch: false,
      }),
    );
    expect(useHostingNav.getState().pullRequest).toMatchObject({ number: 9, repoId: 7 });
    expect(usePullUi.getState().create).toBeNull();
  });
});
