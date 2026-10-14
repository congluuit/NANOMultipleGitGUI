import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { CommitGuardDialog } from "./CommitGuardDialog";
import { PushGuardDialog } from "./PushGuardDialog";
import { MB, MISMATCH, WORK_SUGGESTION, blob, commitReport, pushReport } from "./testData";

beforeAll(() => {
  initI18n("vi");
});

describe("CommitGuardDialog", () => {
  function setup(report = MISMATCH) {
    const props = { onSwitch: vi.fn(), onCommitAnyway: vi.fn(), onCancel: vi.fn() };
    render(<CommitGuardDialog report={report} accountLabel="Cá nhân" {...props} />);
    return props;
  }

  it("renders nothing when closed", () => {
    render(
      <CommitGuardDialog
        report={null}
        onSwitch={vi.fn()}
        onCommitAnyway={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows the yellow warnings: mismatch, unverified email, signing", () => {
    setup();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Kiểm tra trước khi commit")).toBeInTheDocument();
    expect(
      within(dialog).getByText("Email hoangnv@gmail.com không khớp với repo này"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText("Theo quy tắc, repo này thuộc tài khoản Công ty."),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("Email mong đợi: hoang@acme.dev")).toBeInTheDocument();
    expect(
      within(dialog).getByText(/chưa được xác minh trên tài khoản Cá nhân/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("Thiếu khóa ký commit")).toBeInTheDocument();
    expect(within(dialog).getAllByRole("alert")).toHaveLength(3);
  });

  it("switches, commits anyway or cancels", async () => {
    const user = userEvent.setup();
    const props = setup();
    await user.click(screen.getByRole("button", { name: "Đổi sang tài khoản Công ty" }));
    expect(props.onSwitch).toHaveBeenCalledWith(WORK_SUGGESTION);
    await user.click(screen.getByRole("button", { name: "Vẫn commit" }));
    expect(props.onCommitAnyway).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(props.onCancel).toHaveBeenCalled();
  });

  it("explains an unbound repo", () => {
    setup(commitReport({ accountId: null, findings: [{ kind: "unbound", suggestions: [] }] }));
    expect(screen.getByText("Repo này chưa gắn tài khoản")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Đổi sang/ })).not.toBeInTheDocument();
  });
});

describe("PushGuardDialog", () => {
  function setup(
    report = pushReport(),
    rewrite: Parameters<typeof PushGuardDialog>[0]["rewrite"] = null,
  ) {
    const props = {
      onFixAuthors: vi.fn(),
      onMigrate: vi.fn(),
      onUndo: vi.fn(),
      onPush: vi.fn(),
      onCancel: vi.fn(),
    };
    render(<PushGuardDialog report={report} rewrite={rewrite} {...props} />);
    return props;
  }

  it("lists mismatched authors and fixes them", async () => {
    const user = userEvent.setup();
    const props = setup();
    expect(screen.getByText("2 commit sẽ được push lên origin/main")).toBeInTheDocument();
    expect(screen.getByText("1 commit có tác giả khác identity hiện tại")).toBeInTheDocument();
    expect(screen.getByText("hoangnv@gmail.com")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sửa tác giả commit cuối" }));
    expect(props.onFixAuthors).toHaveBeenCalledWith("amendLast");
    await user.click(screen.getByRole("button", { name: "Sửa tác giả 2 commit chưa push" }));
    expect(props.onFixAuthors).toHaveBeenCalledWith("rewriteUnpushed");
    await user.click(screen.getByRole("button", { name: "Vẫn push" }));
    expect(props.onPush).toHaveBeenCalled();
  });

  it("disables fixes off the branch and blocks a denied push", () => {
    setup(
      pushReport({
        isCurrent: false,
        blocked: true,
        findings: [
          { kind: "authorMismatch", count: 1, email: "hoang@acme.dev" },
          { kind: "permissionDenied", accountLabel: "Công ty", level: "read" },
        ],
      }),
    );
    expect(screen.getByText(/Tài khoản Công ty không có quyền push/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sửa tác giả commit cuối" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Vẫn push" })).toBeDisabled();
  });

  it("moves heavy blobs to LFS and explains pushed ones", async () => {
    const user = userEvent.setup();
    const props = setup(
      pushReport({
        authorMismatch: [],
        blocked: true,
        heavyBlobs: [
          blob("Assets/intro.mp4", 120 * MB),
          blob("old/huge.zip", 70 * MB, { inSharedCommit: true }),
        ],
        findings: [
          { kind: "heavyBlobs", block: 1, warn: 0 },
          { kind: "heavyBlobsShared", count: 1 },
        ],
      }),
    );
    expect(screen.getByText("1 file vượt giới hạn của provider")).toBeInTheDocument();
    expect(screen.getByText("Assets/intro.mp4")).toBeInTheDocument();
    expect(screen.getByText("Vượt giới hạn")).toBeInTheDocument();
    expect(screen.getByText("File nặng nằm trong commit đã push")).toBeInTheDocument();
    expect(screen.getByText(/không tự viết lại lịch sử đã chia sẻ/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Chuyển sang LFS" }));
    expect(props.onMigrate).toHaveBeenCalledWith(["Assets/intro.mp4"]);
    expect(screen.getByRole("button", { name: "Vẫn push" })).toBeDisabled();
  });

  it("shows the backup ref with Undo after a rewrite", async () => {
    const user = userEvent.setup();
    const props = setup(pushReport({ findings: [], authorMismatch: [] }), {
      branch: "main",
      backupRef: "refs/nanomgg/backup/1700000000",
      oldHead: "a",
      newHead: "b",
      rewritten: 2,
    });
    expect(
      screen.getByText("Đã viết lại 2 commit. Bản sao lưu: refs/nanomgg/backup/1700000000"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hoàn tác" }));
    expect(props.onUndo).toHaveBeenCalledWith("refs/nanomgg/backup/1700000000");
    // Nothing left: the main button is a plain "Push".
    expect(screen.getByRole("button", { name: "Push" })).toBeEnabled();
  });
});
