import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { commands, type GraphRow, type Outcome } from "@/ipc";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { useUi } from "@/stores/ui";
import { CommitMenuContent } from "./CommitMenu";
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

function wrap(ui: React.ReactNode) {
  return render(<QueryClientProvider client={new QueryClient()}>{ui}</QueryClientProvider>);
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
