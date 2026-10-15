import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { commands, type GraphRow, type HostedRemote, type Outcome, type RepoRefs } from "@/ipc";
import { issueKeys } from "@/features/issues/queries";
import { useRebaseDialog } from "@/features/rebase/store";
import { branchPushPlan } from "@/features/remote/plan";
import { repoKeys } from "@/features/repos/queries";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { useUi } from "@/stores/ui";
import { webBranchUrl, webCommitUrl } from "./branchActions";
import { CommitMenuContent } from "./CommitMenu";
import { RefMenuContent } from "./RefMenu";
import { conflictText, failureText, runOutcome } from "./outcome";
import { RefDialogs } from "./RefDialogs";
import { useRefDialog } from "./store";

const row: GraphRow = {
  index: 0,
  hash: "abcdef1234567890abcdef1234567890abcdef12",
  subject: "feat: add login",
  authorName: "A",
  authorEmail: "a@x.dev",
  authorTime: 0,
  refs: [
    "HEAD -> refs/heads/main",
    "refs/heads/topic",
    "refs/remotes/origin/main",
    "tag: refs/tags/v1",
  ],
  isMerge: false,
  lane: 0,
  color: 0,
  segments: [],
};

const refs: RepoRefs = {
  head: { branch: "main", oid: row.hash, detached: false, unborn: false },
  local: [
    {
      name: "main",
      oid: row.hash,
      upstream: "origin/main",
      ahead: 0,
      behind: 0,
      upstreamGone: false,
      isHead: true,
    },
    {
      name: "topic",
      oid: row.hash,
      upstream: null,
      ahead: 0,
      behind: 0,
      upstreamGone: false,
      isHead: false,
    },
  ],
  remotes: [
    {
      name: "origin",
      url: "https://github.com/o/r.git",
      branches: [{ name: "main", oid: row.hash }],
    },
  ],
  tags: [],
  stashes: [],
};

const github: HostedRemote = {
  remote: "origin",
  host: "github.com",
  fullName: "o/r",
  accountId: "a",
  family: "github",
  hasToken: true,
};

function wrap(ui: React.ReactNode, client = new QueryClient()) {
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

describe("outcome helpers", () => {
  beforeEach(() => {
    initI18n("vi");
    useUi.setState({ notice: null });
  });

  it("explains rejections with git's message", () => {
    expect(failureText("notFullyMerged", "error: not fully merged")).toBe(
      "Nhánh chưa được merge — error: not fully merged",
    );
    expect(failureText("notFound", "")).toBe("Không tìm thấy");
    expect(conflictText({ conflicts: [] })).toBe("");
    expect(conflictText({ conflicts: ["a", "b"] })).toContain("2 file");
  });

  it("reports outcomes in the notice bar", async () => {
    const done: Outcome<number> = { status: "done", value: 7 };
    await expect(
      runOutcome(
        1,
        () => Promise.resolve(done),
        (v) => `ok ${v}`,
      ),
    ).resolves.toBe(7);
    expect(useUi.getState().notice).toEqual({ kind: "info", message: "ok 7" });

    const rejected: Outcome<number> = { status: "rejected", reason: "localChanges", message: "" };
    await expect(runOutcome(1, () => Promise.resolve(rejected))).resolves.toBeUndefined();
    expect(useUi.getState().notice?.kind).toBe("error");

    await runOutcome(1, () => Promise.reject({ kind: "git", message: "boom" }));
    expect(useUi.getState().notice).toEqual({ kind: "error", message: "boom" });
  });
});

describe("commit context menu", () => {
  beforeEach(() => {
    initI18n("vi");
    useRefDialog.setState({ dialog: null });
  });

  it("lists commit actions and one submenu per ref on the row", async () => {
    const user = userEvent.setup();
    wrap(
      <ContextMenu>
        <ContextMenuTrigger>target</ContextMenuTrigger>
        <CommitMenuContent repoId={1} row={row} />
      </ContextMenu>,
    );
    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("target") });
    expect(await screen.findByText("Tạo nhánh tại đây…")).toBeInTheDocument();
    expect(screen.getByText("Cherry-pick commit")).toBeInTheDocument();
    expect(screen.getByText("Revert commit")).toBeInTheDocument();
    expect(screen.getByText("main")).toBeInTheDocument();
    expect(screen.getByText("topic")).toBeInTheDocument();
    expect(screen.getByText("origin/main")).toBeInTheDocument();
    expect(screen.getByText("v1")).toBeInTheDocument();

    await user.click(screen.getByText("Tạo tag tại đây…"));
    expect(useRefDialog.getState().dialog).toEqual({
      kind: "createTag",
      repoId: 1,
      oid: row.hash,
      label: "abcdef1",
    });
  });
});

describe("branch context menu", () => {
  beforeEach(() => {
    initI18n("en");
    useRefDialog.setState({ dialog: null });
    useRebaseDialog.setState({ dialog: null });
  });

  const openMenu = async (label: Parameters<typeof RefMenuContent>[0]["label"]) => {
    const client = new QueryClient();
    client.setQueryData(repoKeys.refs(1), refs);
    client.setQueryData(issueKeys.remotes(1), [github]);
    const user = userEvent.setup();
    wrap(
      <ContextMenu>
        <ContextMenuTrigger>target</ContextMenuTrigger>
        <RefMenuContent repoId={1} row={row} label={label} />
      </ContextMenu>,
      client,
    );
    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("target") });
    return user;
  };

  it("offers sync, tip-commit, delete and copy actions like other Git clients", async () => {
    const user = await openMenu({ text: "main", kind: "head" });
    for (const text of [
      "Pull (fast-forward if possible)",
      "Push",
      "Set upstream",
      "Create worktree from main…",
      "Create branch here…",
      "Edit commit message…",
      "Revert commit",
      "Start a pull request from main…",
      "Rename main…",
      "Delete origin/main…",
      "Delete main and origin/main…",
      "Copy branch name",
      "Copy link to branch: origin/main",
      "Copy link to this commit on remote: origin",
      "Create annotated tag here…",
    ]) {
      expect(screen.getByText(text)).toBeInTheDocument();
    }
    // The checked-out branch cannot be deleted locally.
    expect(screen.getByText("Delete main…").closest("[role=menuitem]")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await user.click(screen.getByText("Drop commit…"));
    expect(useRebaseDialog.getState().dialog).toMatchObject({
      kind: "interactive",
      base: `${row.hash}~1`,
      preset: { oid: row.hash, action: "drop" },
    });
  });

  it("deletes a branch and its remote branch after confirming", async () => {
    const user = await openMenu({ text: "topic", kind: "branch" });
    expect(screen.queryByText(/Delete topic and/)).not.toBeInTheDocument();
    expect(screen.getByText("Push topic and start a pull request…")).toBeInTheDocument();
    await user.click(screen.getByText("Delete topic…"));
    expect(useRefDialog.getState().dialog).toEqual({
      kind: "deleteBranch",
      repoId: 1,
      name: "topic",
      force: false,
    });
  });

  it("builds push plans and web links for any branch", () => {
    expect(branchPushPlan(refs, "main")).toEqual({
      remote: "origin",
      branch: "main",
      remoteBranch: null,
      setUpstream: false,
      forceWithLease: false,
    });
    expect(branchPushPlan(refs, "topic")).toMatchObject({ remote: "origin", setUpstream: true });
    expect(webBranchUrl(github, "feature/x y")).toBe("https://github.com/o/r/tree/feature/x%20y");
    const gitlab = { ...github, host: "gitlab.acme.dev", family: "gitlab" as const };
    expect(webBranchUrl(gitlab, "main")).toBe("https://gitlab.acme.dev/o/r/-/tree/main");
    expect(webCommitUrl(gitlab, "abc")).toBe("https://gitlab.acme.dev/o/r/-/commit/abc");
  });
});

describe("ref dialogs", () => {
  beforeEach(() => {
    initI18n("vi");
    useRefDialog.setState({ dialog: null });
  });

  it("validates the branch name before creating", async () => {
    const user = userEvent.setup();
    const create = vi
      .spyOn(commands, "branchCreate")
      .mockResolvedValue({ status: "done", value: null });
    wrap(<RefDialogs />);
    act(() =>
      useRefDialog
        .getState()
        .open({ kind: "createBranch", repoId: 1, start: row.hash, label: "abcdef1" }),
    );
    const button = await screen.findByRole("button", { name: "Tạo" });
    expect(button).toBeDisabled();
    await user.type(screen.getByLabelText("Tên nhánh"), "bad name");
    expect(button).toBeDisabled();
    await user.clear(screen.getByLabelText("Tên nhánh"));
    await user.type(screen.getByLabelText("Tên nhánh"), "feature/login");
    await user.click(button);
    expect(create).toHaveBeenCalledWith(1, "feature/login", row.hash, true);
    expect(useRefDialog.getState().dialog).toBeNull();
    create.mockRestore();
  });

  it("asks before a hard reset", async () => {
    wrap(<RefDialogs />);
    act(() =>
      useRefDialog.getState().open({
        kind: "resetHard",
        repoId: 1,
        oid: row.hash,
        label: "abcdef1",
        branch: "main",
      }),
    );
    expect(await screen.findByText("Reset hard main về abcdef1?")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reset hard" })).toBeInTheDocument();
  });
});
