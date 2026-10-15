import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import i18n from "i18next";
import { useState } from "react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { accountKeys } from "@/features/accounts/queries";
import { initI18n } from "@/i18n";
import {
  commands,
  type Account,
  type HostedRemote,
  type IssueCommentList,
  type IssueDetail,
  type IssueListResult,
  type IssueSummary,
  type Provider,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useHostingNav } from "@/stores/hosting";
import { describeIssueError, describeProviderError } from "./errors";
import { useIssueRefSuggest } from "./IssueRefSuggest";
import { IssueView } from "./IssueView";
import { applyRef, fold, hostName, matchIssues, refTrigger, sameProject, splitRefs } from "./model";
import { issueKeys } from "./queries";
import { SafeText } from "./SafeText";

const t = (key: string, vars?: Record<string, unknown>) =>
  i18n.t(key, vars as Record<string, string>);

const account = (id: string, providerId = "gh"): Account =>
  ({
    id,
    username: id,
    providerId,
    color: "#3366ff",
    label: id === "acc1" ? "Công ty" : null,
    displayName: null,
    secrets: ["token"],
    sortOrder: 0,
  }) as unknown as Account;

const issue = (number: number, title: string, extra: Partial<IssueSummary> = {}): IssueSummary => ({
  number,
  title,
  state: "open",
  author: { username: "alice", displayName: null, avatarUrl: null },
  labels: [{ name: "bug", color: "#d73a4a", description: null }],
  assignees: [],
  comments: 1,
  createdAt: 1_700_000_000,
  updatedAt: 1_700_000_000,
  webUrl: `https://github.com/acme/game/issues/${number}`,
  isPullRequest: false,
  ...extra,
});

const remote: HostedRemote = {
  remote: "origin",
  host: "github.com",
  fullName: "acme/game",
  accountId: "acc1",
  family: "github",
  hasToken: true,
};

beforeAll(() => {
  initI18n("vi");
});

afterEach(() => {
  queryClient.clear();
  vi.restoreAllMocks();
  useHostingNav.setState({ issue: null, pullRequest: null });
});

describe("issues model", () => {
  it("splits refs and https links out of untrusted text", () => {
    const parts = splitRefs("Fix #12 and !3, see https://x.dev/a). Not a#4 or &#5", "github");
    expect(parts).toEqual([
      { type: "text", text: "Fix " },
      { type: "ref", raw: "#12", number: 12, mergeRequest: false },
      { type: "text", text: " and !3, see " },
      { type: "url", url: "https://x.dev/a" },
      { type: "text", text: "). Not a#4 or &#5" },
    ]);
    expect(splitRefs("MR !3", "gitlab")[1]).toEqual({
      type: "ref",
      raw: "!3",
      number: 3,
      mergeRequest: true,
    });
    expect(splitRefs("javascript:alert(1) #0", "github")).toEqual([
      { type: "text", text: "javascript:alert(1) #0" },
    ]);
  });

  it("detects the # trigger and inserts the reference", () => {
    expect(refTrigger("Fix #1", 6)).toEqual({ start: 4, query: "1" });
    expect(refTrigger("#", 1)).toEqual({ start: 0, query: "" });
    expect(refTrigger("abc#1", 5)).toBeNull();
    expect(refTrigger("Fix #1 done", 11)).toBeNull();
    const trig = refTrigger("Fix #1 tail", 6)!;
    expect(applyRef("Fix #1 tail", trig, 6, "Closes #12")).toEqual({
      value: "Fix Closes #12 tail",
      caret: 15,
    });
    expect(applyRef("#", { start: 0, query: "" }, 1, "#7")).toEqual({ value: "#7 ", caret: 3 });
  });

  it("matches issues by number or accent-insensitive title", () => {
    const list = [issue(12, "Sửa lỗi đăng nhập"), issue(120, "Crash"), issue(3, "Other")];
    expect(matchIssues(list, "12").map((i) => i.number)).toEqual([12, 120]);
    expect(matchIssues(list, "dang nhap").map((i) => i.number)).toEqual([12]);
    expect(matchIssues(list, "").length).toBe(3);
    expect(fold("Đăng")).toBe("dang");
  });

  it("compares projects by host name and full name", () => {
    expect(hostName("https://GitLab.acme.dev:8443/x")).toBe("gitlab.acme.dev");
    expect(hostName("gitlab.acme.dev:2222")).toBe("gitlab.acme.dev");
    expect(hostName("git@github.com")).toBe("github.com");
    expect(sameProject(remote, "github.com", "ACME/Game")).toBe(true);
    expect(sameProject(remote, "gitlab.com", "acme/game")).toBe(false);
  });
});

describe("issue errors", () => {
  it("explains missing scopes, 404 on private projects and rate limits", () => {
    const scopes = describeProviderError(
      t as never,
      {
        kind: "forbidden",
        message: "x",
        hint: { kind: "missingScopes", required: ["repo"], granted: [] },
      },
      "Công ty",
    );
    expect(scopes.fix).toBe("reauth");
    expect(scopes.message).toContain("repo");
    const notFound = describeProviderError(
      t as never,
      { kind: "notFound", message: "Not Found", hint: null },
      "Công ty",
    );
    expect(notFound.fix).toBe("switchAccount");
    expect(notFound.message).toContain("Công ty");
    const limited = describeProviderError(
      t as never,
      { kind: "rateLimited", resetAt: null, retryAfterSecs: 60 },
      "x",
      "vi",
      0,
    );
    expect(limited.fix).toBe("wait");
    expect(limited.message).toMatch(/\d/);
    const saml = describeProviderError(
      t as never,
      {
        kind: "forbidden",
        message: "",
        hint: { kind: "samlSsoRequired", url: "javascript:alert(1)" },
      },
      "x",
    );
    expect(saml.url).toBeUndefined();
    expect(
      describeIssueError(
        t as never,
        { kind: "provider", message: "", error: { kind: "cancelled" } },
        "x",
      ).cancelled,
    ).toBe(true);
    expect(
      describeIssueError(
        t as never,
        { kind: "app", message: "no remote", error: { kind: "unsupported", message: "no remote" } },
        "x",
      ).hint,
    ).toBeTruthy();
  });
});

describe("SafeText", () => {
  it("never renders provider HTML and turns refs into buttons", () => {
    const onRef = vi.fn();
    const { container } = render(
      <SafeText text={'<img src=x onerror="alert(1)"> fixes #7'} family="github" onRef={onRef} />,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain("<img src=x");
    fireEvent.click(screen.getByRole("button", { name: "#7" }));
    expect(onRef).toHaveBeenCalledWith(expect.objectContaining({ number: 7 }));
  });
});

function seedAccounts() {
  queryClient.setQueryData(accountKeys.list, [account("acc1"), account("acc2")]);
  queryClient.setQueryData(accountKeys.providers, [
    { id: "gh", kind: "github", baseUrl: "https://github.com" } as Provider,
  ]);
}

describe("IssueView", () => {
  const ref = { accountId: "acc1", fullName: "acme/game", number: 7, repoId: 1 };
  const detail: IssueDetail = {
    issue: issue(7, "Crash khi mở", {
      labels: [{ name: "bug", color: "#d73a4a", description: null }],
    }),
    body: "Steps <b>bold</b> see #8",
    closedAt: null,
    closedBy: null,
    locked: false,
  };
  const comments: IssueCommentList = {
    comments: [
      {
        id: "1",
        author: { username: "bob", displayName: null, avatarUrl: null },
        body: "Me too",
        createdAt: 1_700_000_100,
        updatedAt: null,
        webUrl: null,
        system: false,
      },
    ],
    truncated: false,
  };

  it("shows the issue as text and comments as the chosen account", async () => {
    seedAccounts();
    queryClient.setQueryData(issueKeys.detail(ref), detail);
    queryClient.setQueryData(issueKeys.comments(ref), comments);
    const create = vi
      .spyOn(commands, "issueCommentCreate")
      .mockResolvedValue(comments.comments[0]!);
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <IssueView issueRef={ref} />
      </QueryClientProvider>,
    );
    expect(screen.getByRole("heading", { name: /Crash khi mở/ })).toBeInTheDocument();
    expect(screen.getByText(/Steps <b>bold<\/b>/)).toBeInTheDocument();
    expect(screen.getByText("Me too")).toBeInTheDocument();
    expect(screen.getAllByText("Công ty").length).toBeGreaterThan(0);
    await user.type(screen.getByLabelText(/Viết bình luận/), "Đã sửa");
    await user.click(screen.getByRole("button", { name: "Bình luận" }));
    await waitFor(() => expect(create).toHaveBeenCalledWith("acc1", "acme/game", 7, "Đã sửa"));
  });

  it("offers another account when the project is not found", async () => {
    seedAccounts();
    vi.spyOn(commands, "issueGet").mockRejectedValue({
      kind: "provider",
      message: "not found",
      error: { kind: "notFound", message: "Not Found", hint: null },
    });
    vi.spyOn(commands, "issueComments").mockResolvedValue(comments);
    // jsdom has no Tauri: run the query by hand.
    await queryClient
      .fetchQuery({
        queryKey: issueKeys.detail(ref),
        queryFn: () => commands.issueGet("acc1", "acme/game", 7),
      })
      .catch(() => undefined);
    render(
      <QueryClientProvider client={queryClient}>
        <IssueView issueRef={ref} />
      </QueryClientProvider>,
    );
    expect(await screen.findByText(/không thấy dự án/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Thử tài khoản khác" })).toBeInTheDocument();
  });
});

function SuggestHarness() {
  const [value, setValue] = useState("");
  const s = useIssueRefSuggest<HTMLInputElement>(1, value, setValue);
  return (
    <div className="relative">
      <input
        aria-label="summary"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => s.onKeyDown(e)}
        onSelect={s.onCaret}
      />
      {s.popup}
    </div>
  );
}

describe("# suggestions", () => {
  it("lists open issues after # and inserts the number", async () => {
    queryClient.setQueryData(issueKeys.remotes(1), [remote]);
    const result: IssueListResult = {
      page: { items: [issue(12, "Login lỗi"), issue(30, "Crash")], nextPage: null, total: null },
      accountId: "acc1",
      fullName: "acme/game",
      remote: "origin",
    };
    queryClient.setQueryData(issueKeys.open(1), result);
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <SuggestHarness />
      </QueryClientProvider>,
    );
    const input = screen.getByLabelText("summary");
    await user.type(input, "Fix #log");
    expect(screen.getByRole("option", { name: /Login lỗi/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /Crash/ })).toBeNull();
    await user.keyboard("{Enter}");
    expect(input).toHaveValue("Fix #12 ");
    await user.type(input, "#3");
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(input).toHaveValue("Fix #12 Closes #30 ");
  });
});
