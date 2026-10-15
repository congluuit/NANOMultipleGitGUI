import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { accountKeys } from "@/features/accounts/queries";
import { issueKeys } from "@/features/issues/queries";
import { initI18n } from "@/i18n";
import type {
  Account,
  HostedRemote,
  Provider,
  RepoTab,
  WorkItem,
  WorkLists,
  WorkOverview,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useHostingNav } from "@/stores/hosting";
import { useWorkspace } from "@/stores/workspace";
import { findOpenRepo, groupRows, groupTotal, reviewCount } from "./model";
import { MyWorkHost } from "./MyWorkPanel";
import { myworkKeys } from "./queries";
import { useMyWorkUi } from "./store";

const item = (fullName: string, number: number, extra: Partial<WorkItem> = {}): WorkItem => ({
  kind: "pullRequest",
  fullName,
  number,
  title: `Item ${number}`,
  author: "bob",
  authorAvatar: null,
  updatedAt: 1_700_000_000 + number,
  draft: false,
  labels: [],
  comments: 0,
  webUrl: `https://github.com/${fullName}/pull/${number}`,
  ...extra,
});

const lists = (review: WorkItem[], assigned: WorkItem[] = [], total?: number): WorkLists => ({
  reviewRequested: { items: review, total: total ?? review.length },
  authored: { items: [], total: 0 },
  assigned: { items: assigned, total: assigned.length },
});

const account = (id: string, label: string, providerId: string): Account =>
  ({
    id,
    username: id,
    providerId,
    color: id === "work" ? "#ff0000" : "#00ff00",
    label,
    displayName: null,
    secrets: ["token"],
    sortOrder: 0,
  }) as unknown as Account;

const overview: WorkOverview = {
  accounts: [
    {
      accountId: "work",
      lists: lists([item("acme/game", 5)], [item("acme/game", 9, { kind: "issue" })], 7),
      error: null,
      fetchedAt: 1,
    },
    {
      accountId: "home",
      lists: lists([item("me/tool", 2, { draft: true })]),
      error: null,
      fetchedAt: 1,
    },
    {
      accountId: "old",
      lists: null,
      error: {
        kind: "provider",
        message: "401",
        error: { kind: "unauthorized", message: "Bad credentials" },
      },
      fetchedAt: null,
    },
  ],
  withoutToken: ["ssh"],
  cancelled: false,
};

beforeAll(() => {
  initI18n("vi");
});

afterEach(() => {
  queryClient.clear();
  useMyWorkUi.setState({ open: false, accountId: null });
  useHostingNav.setState({ issue: null, pullRequest: null });
});

describe("my work model", () => {
  it("merges accounts, filters by account and counts reviews", () => {
    expect(groupRows(overview.accounts, "reviewRequested", null).map((r) => r.item.number)).toEqual(
      [5, 2],
    );
    expect(groupRows(overview.accounts, "reviewRequested", "home")).toHaveLength(1);
    expect(groupTotal(overview.accounts, "reviewRequested")).toBe(8);
    expect(reviewCount(overview)).toBe(8);
    expect(reviewCount(undefined)).toBe(0);
  });

  it("finds the open tab of a project, active tab first", () => {
    const r = (fullName: string, host = "github.com") =>
      [{ host, fullName, remote: "origin" }] as HostedRemote[];
    const tabs = [
      { id: 1, remotes: r("acme/game") },
      { id: 2, remotes: r("acme/game") },
      { id: 3, remotes: undefined },
    ];
    expect(findOpenRepo(tabs, 2, "github.com", "ACME/game")).toBe(2);
    expect(findOpenRepo(tabs, 3, "github.com", "acme/game")).toBe(1);
    expect(findOpenRepo(tabs, 1, "gitlab.com", "acme/game")).toBeNull();
  });
});

describe("MyWorkPanel", () => {
  it("shows groups with account colors, per-account errors, and opens rows in the right tab", async () => {
    queryClient.setQueryData(accountKeys.list, [
      account("work", "Công ty", "gh"),
      account("home", "Cá nhân", "gh"),
      account("old", "Cũ", "gh"),
    ]);
    queryClient.setQueryData(accountKeys.providers, [
      { id: "gh", kind: "github", baseUrl: "https://github.com" } as Provider,
    ]);
    queryClient.setQueryData(myworkKeys.all, overview);
    useWorkspace.setState({
      tabs: [{ id: 4, name: "game", path: "/g", missing: false } as RepoTab],
      activeId: 4,
    });
    queryClient.setQueryData(issueKeys.remotes(4), [
      {
        remote: "origin",
        host: "github.com",
        fullName: "acme/game",
        accountId: "work",
        family: "github",
        hasToken: true,
      },
    ]);
    useMyWorkUi.setState({ open: true });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <MyWorkHost />
      </QueryClientProvider>,
    );
    expect(screen.getByRole("region", { name: "Cần tôi review" })).toBeInTheDocument();
    expect(screen.getByText(/Token của Cũ không còn dùng được/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Đăng nhập lại" })).toBeInTheDocument();
    expect(screen.getByText(/1 tài khoản chưa có token/)).toBeInTheDocument();
    expect(screen.getByText("Nháp")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Item 9/ }));
    expect(useHostingNav.getState().issue).toEqual({
      accountId: "work",
      fullName: "acme/game",
      number: 9,
      repoId: 4,
    });
    expect(useMyWorkUi.getState().open).toBe(false);
  });

  it("opens a project that is not open without a repository", async () => {
    queryClient.setQueryData(accountKeys.list, [account("home", "Cá nhân", "gh")]);
    queryClient.setQueryData(accountKeys.providers, [
      { id: "gh", kind: "github", baseUrl: "https://github.com" } as Provider,
    ]);
    queryClient.setQueryData(myworkKeys.all, { ...overview, withoutToken: [] });
    useWorkspace.setState({
      tabs: [{ id: 4, name: "game", path: "/g", missing: false } as RepoTab],
      activeId: 4,
    });
    queryClient.setQueryData(issueKeys.remotes(4), []);
    useMyWorkUi.setState({ open: true, accountId: "home" });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <MyWorkHost />
      </QueryClientProvider>,
    );
    expect(screen.queryByText(/Item 5/)).toBeNull();
    await user.click(screen.getByRole("button", { name: /Item 2/ }));
    expect(useHostingNav.getState().pullRequest).toMatchObject({ number: 2, repoId: null });
  });
});
