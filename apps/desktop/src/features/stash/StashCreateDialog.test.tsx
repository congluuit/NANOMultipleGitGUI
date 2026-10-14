import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { StashDetails } from "@/ipc";
import { StashCreateDialog } from "./StashCreateDialog";
import { StashFileList } from "./StashFilesDialog";

beforeAll(() => {
  initI18n("vi");
});

describe("StashCreateDialog", () => {
  it("submits the message and options", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<StashCreateDialog open onSubmit={onSubmit} onCancel={() => {}} />);
    expect(screen.getByRole("dialog", { name: "Stash thay đổi" })).toBeInTheDocument();
    const untracked = screen.getByRole("checkbox", { name: "Gồm cả file mới chưa track" });
    expect(untracked).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Giữ lại phần đã stage" })).not.toBeChecked();

    await user.type(screen.getByLabelText("Ghi chú (không bắt buộc)"), "đang sửa login");
    await user.click(untracked);
    await user.click(screen.getByRole("checkbox", { name: "Giữ lại phần đã stage" }));
    await user.click(screen.getByRole("button", { name: "Stash" }));
    expect(onSubmit).toHaveBeenCalledWith({
      message: "đang sửa login",
      includeUntracked: false,
      keepIndex: true,
    });
  });

  it("submits with Enter and cancels", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    const onCancel = vi.fn();
    render(<StashCreateDialog open onSubmit={onSubmit} onCancel={onCancel} />);
    await user.type(screen.getByLabelText("Ghi chú (không bắt buộc)"), "x{Enter}");
    expect(onSubmit).toHaveBeenCalledWith({
      message: "x",
      includeUntracked: true,
      keepIndex: false,
    });
    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("does not submit while busy", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<StashCreateDialog open busy onSubmit={onSubmit} onCancel={() => {}} />);
    expect(screen.getByRole("button", { name: "Stash" })).toBeDisabled();
    await user.type(screen.getByLabelText("Ghi chú (không bắt buộc)"), "{Enter}");
    expect(onSubmit).not.toHaveBeenCalled();
  });
});

describe("StashFileList", () => {
  const details: StashDetails = {
    index: 0,
    oid: "a".repeat(40),
    base: "b".repeat(40),
    message: "On main: wip",
    time: 0,
    files: [
      { status: "modified", path: "src/app.ts", oldPath: null, additions: 3, deletions: 1 },
      { status: "added", path: "logo.png", oldPath: null, additions: null, deletions: null },
    ],
    untracked: [{ status: "added", path: "notes.txt", oldPath: null, additions: 2, deletions: 0 }],
    additions: 5,
    deletions: 1,
  };

  it("lists tracked changes and untracked files separately", () => {
    render(<StashFileList details={details} />);
    expect(screen.getByText("Thay đổi (2)")).toBeInTheDocument();
    expect(screen.getByText("File mới chưa track (1)")).toBeInTheDocument();
    expect(screen.getByText("app.ts")).toBeInTheDocument();
    expect(screen.getByText("+3")).toBeInTheDocument();
    expect(screen.getByText("−1")).toBeInTheDocument();
    expect(screen.getByText("nhị phân")).toBeInTheDocument();
    expect(screen.getByText("notes.txt")).toBeInTheDocument();
  });

  it("says when a stash is empty", () => {
    render(<StashFileList details={{ ...details, files: [], untracked: [] }} />);
    expect(screen.getByText("Stash này không có file nào")).toBeInTheDocument();
  });
});
