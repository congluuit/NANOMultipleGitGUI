import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { OperationBannerView } from "@/features/commits/OperationBanner";
import { initI18n } from "@/i18n";
import type { RebaseCommit, RebasePreview } from "@/ipc";
import { InteractiveRebaseView } from "./InteractiveRebaseDialog";
import { RebaseSummary } from "./RebaseDialog";

beforeAll(() => {
  initI18n("vi");
});

const commit = (n: string, extra: Partial<RebaseCommit> = {}): RebaseCommit => ({
  oid: n.repeat(40),
  shortOid: n.repeat(7),
  subject: `commit ${n}`,
  message: `commit ${n}`,
  authorName: "A",
  time: 0,
  pushed: false,
  isMerge: false,
  ...extra,
});

const preview = (commits: RebaseCommit[], extra: Partial<RebasePreview> = {}): RebasePreview => ({
  branch: "topic",
  isCurrent: true,
  head: commits[commits.length - 1]?.oid ?? "",
  onto: "0".repeat(40),
  commits,
  total: commits.length,
  pushed: commits.filter((c) => c.pushed).length,
  merges: 0,
  dirty: false,
  upToDate: false,
  fastForward: false,
  ...extra,
});

const order = () =>
  within(screen.getByRole("list", { name: "Danh sách commit" }))
    .getAllByRole("listitem")
    .map((li) => li.getAttribute("data-oid")![0]);

describe("InteractiveRebaseView", () => {
  it("reorders with Alt+arrows, keeps focus on the moved row and submits the plan", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <InteractiveRebaseView
        preview={preview([commit("a"), commit("b"), commit("c")])}
        baseLabel="1234567"
        onSubmit={onSubmit}
        onCancel={() => {}}
      />,
    );
    const start = screen.getByRole("button", { name: "Bắt đầu rebase" });
    expect(start).toBeDisabled(); // nothing changed yet
    expect(screen.getByText("Chưa có thay đổi nào so với hiện tại.")).toBeInTheDocument();

    const rowA = screen.getByRole("listitem", { name: "aaaaaaa commit a" });
    rowA.focus();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(order()).toEqual(["b", "a", "c"]);
    expect(screen.getByRole("listitem", { name: "aaaaaaa commit a" })).toHaveFocus();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(order()).toEqual(["b", "c", "a"]);
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(order()).toEqual(["b", "a", "c"]);
    // Plain arrows do not move.
    await user.keyboard("{ArrowUp}");
    expect(order()).toEqual(["b", "a", "c"]);

    await user.click(start);
    expect(onSubmit).toHaveBeenCalledWith({
      steps: [
        { oid: "b".repeat(40), action: "pick", message: null },
        { oid: "a".repeat(40), action: "pick", message: null },
        { oid: "c".repeat(40), action: "pick", message: null },
      ],
      autostash: false,
      allowPushed: false,
    });
  });

  it("changes actions, rewords inline and previews the result", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <InteractiveRebaseView
        preview={preview([commit("a"), commit("b"), commit("c")])}
        baseLabel="1234567"
        onSubmit={onSubmit}
        onCancel={() => {}}
      />,
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Hành động cho aaaaaaa" }),
      "reword",
    );
    const editor = screen.getByLabelText("Message mới cho aaaaaaa");
    expect(editor).toHaveValue("commit a");
    await user.clear(editor);
    await user.type(editor, "Sửa lại message");
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Hành động cho bbbbbbb" }),
      "fixup",
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Hành động cho ccccccc" }),
      "drop",
    );

    const result = screen.getByRole("list", { name: "Kết quả: 1 commit" });
    expect(within(result).getByText("Sửa lại message")).toBeInTheDocument();
    expect(within(result).getByText(/\+1 commit gộp vào/)).toBeInTheDocument();
    expect(screen.getByText("1 commit bị bỏ")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Bắt đầu rebase" }));
    expect(onSubmit.mock.calls[0]![0].steps).toEqual([
      { oid: "a".repeat(40), action: "reword", message: "Sửa lại message" },
      { oid: "b".repeat(40), action: "fixup", message: null },
      { oid: "c".repeat(40), action: "drop", message: null },
    ]);
  });

  it("blocks squash on the first commit, empty messages and unconfirmed pushed rewrites", async () => {
    const user = userEvent.setup();
    render(
      <InteractiveRebaseView
        preview={preview([commit("a", { pushed: true }), commit("b")])}
        baseLabel="1234567"
        onSubmit={() => {}}
        onCancel={() => {}}
      />,
    );
    const start = screen.getByRole("button", { name: "Bắt đầu rebase" });
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Hành động cho aaaaaaa" }),
      "squash",
    );
    expect(screen.getByRole("alert")).toHaveTextContent("không thể squash");
    expect(start).toBeDisabled();
    // Touching the pushed commit asks for confirmation.
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Hành động cho aaaaaaa" }),
      "reword",
    );
    await user.clear(screen.getByLabelText("Message mới cho aaaaaaa"));
    expect(screen.getByRole("alert")).toHaveTextContent("đang trống");
    await user.type(screen.getByLabelText("Message mới cho aaaaaaa"), "A");
    expect(screen.getByText(/đã có trên remote/)).toBeInTheDocument();
    expect(start).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: /vẫn viết lại/ }));
    expect(start).toBeEnabled();
  });

  it("explains that merge commits are not supported", () => {
    render(
      <InteractiveRebaseView
        preview={preview([commit("a"), commit("m", { isMerge: true })])}
        baseLabel="1234567"
        onSubmit={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("merge commit");
  });
});

describe("RebaseSummary", () => {
  it("lists commits, warns about pushed commits and offers autostash", async () => {
    const user = userEvent.setup();
    const onAllowPushed = vi.fn();
    const onAutostash = vi.fn();
    render(
      <RebaseSummary
        preview={preview([commit("a", { pushed: true }), commit("b")], {
          dirty: true,
          isCurrent: false,
        })}
        ontoLabel="main"
        allowPushed={false}
        autostash={false}
        onAllowPushed={onAllowPushed}
        onAutostash={onAutostash}
      />,
    );
    expect(screen.getByText("2 commit sẽ được viết lại:")).toBeInTheDocument();
    expect(screen.getByText(/app sẽ checkout nó trước/)).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: /vẫn viết lại/ }));
    expect(onAllowPushed).toHaveBeenCalledWith(true);
    await user.click(screen.getByRole("checkbox", { name: /autostash/ }));
    expect(onAutostash).toHaveBeenCalledWith(true);
  });

  it("says when there is nothing to do", () => {
    render(
      <RebaseSummary
        preview={preview([], { upToDate: true })}
        ontoLabel="main"
        allowPushed={false}
        autostash={false}
        onAllowPushed={() => {}}
        onAutostash={() => {}}
      />,
    );
    expect(screen.getByText("topic đã chứa main — không có gì để rebase.")).toBeInTheDocument();
  });
});

describe("OperationBannerView", () => {
  it("shows the rebase step with skip, continue and abort", () => {
    render(
      <OperationBannerView
        repoId={1}
        state={{ operation: "rebase", conflicts: ["a.txt"], step: 2, totalSteps: 5 }}
      />,
    );
    expect(screen.getByText("Rebase bước 2/5")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Bỏ qua commit này" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Tiếp tục" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Hủy" })).toBeEnabled();
  });

  it("has no skip button for a merge", () => {
    render(
      <OperationBannerView
        repoId={1}
        state={{ operation: "merge", conflicts: [], step: null, totalSteps: null }}
      />,
    );
    expect(screen.queryByRole("button", { name: "Bỏ qua commit này" })).toBeNull();
    expect(screen.getByRole("button", { name: "Tiếp tục" })).toBeEnabled();
  });
});
