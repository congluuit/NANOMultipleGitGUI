import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { makeContext, seedAccounts } from "@/features/accounts/switcher/testData";
import { initI18n } from "@/i18n";
import type { LfsFailure, LfsTransferProgress } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useWorkspace } from "@/stores/workspace";
import { setRetry, useTransferQueue } from "./store";
import { TransferQueueWidget } from "./TransferQueueWidget";

const ipc = vi.hoisted(() => ({ transferCancel: vi.fn(async () => true) }));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, transferCancel: ipc.transferCancel } };
});

const MB = 1024 * 1024;
const GB = 1024 * MB;

beforeAll(() => {
  initI18n("vi");
});

beforeEach(() => {
  useTransferQueue.setState({ items: {}, order: [] });
  useWorkspace.setState({
    tabs: [
      { id: 1, path: "C:/r/game", name: "game-assets", missing: false },
      { id: 2, path: "C:/r/api", name: "backend-api", missing: false },
    ],
  });
  queryClient.clear();
  // Repo 1 is bound to "Công ty" (Work); repo 2 has no context loaded.
  seedAccounts(queryClient, { contexts: [makeContext({ repoId: 1 })] });
  ipc.transferCancel.mockClear();
});

const progress = (over: Partial<LfsTransferProgress>): LfsTransferProgress => ({
  repoId: 1,
  opId: "lfs-up",
  direction: "upload",
  filesDone: 5,
  filesTotal: 12,
  bytesDone: 760 * MB,
  bytesTotal: 1.8 * GB,
  currentFile: "Assets/intro.mp4",
  bytesPerSec: 5 * MB,
  etaSecs: 200,
  active: [{ name: "Assets/intro.mp4", bytesDone: 100 * MB, bytesTotal: 312 * MB }],
  done: false,
  ...over,
});

const failure = (over: Partial<LfsFailure>): LfsFailure => ({
  kind: "other",
  message: "batch response: Authorization error",
  retryable: false,
  billingUrl: null,
  settingsUrl: null,
  limitBytes: null,
  object: null,
  ...over,
});

describe("TransferQueueWidget", () => {
  it("renders nothing without transfers", () => {
    const { container } = render(<TransferQueueWidget />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows LFS progress in the status bar and details in the popover", async () => {
    const q = useTransferQueue.getState();
    q.start({ opId: "lfs-up", repoId: 1, kind: "lfsUpload", subject: "origin/main" });
    q.lfsProgress(progress({}));
    q.start({ opId: "fetch-2", repoId: 2, kind: "fetch", subject: null });
    render(<TransferQueueWidget />);

    const trigger = screen.getByRole("status");
    expect(trigger).toHaveTextContent("LFS 41% · 760 MB/1.8 GB");

    await userEvent.click(trigger);
    const row = await screen.findByTestId("transfer-lfs-up");
    expect(row).toHaveTextContent("Upload LFS");
    expect(row).toHaveTextContent("origin/main");
    expect(row).toHaveTextContent(
      "41% · 760 MB / 1.8 GB · 5/12 file · 5 MB/s · còn 3 phút 20 giây",
    );
    expect(row).toHaveTextContent("Assets/intro.mp4");
    expect(row).toHaveTextContent("100 MB / 312 MB");
    // Both repositories are listed, each under its own name.
    expect(screen.getByText("game-assets")).toBeInTheDocument();
    expect(screen.getByText("backend-api")).toBeInTheDocument();
    expect(screen.getByText("Đang chạy: 2")).toBeInTheDocument();

    await userEvent.click(within(row).getByRole("button", { name: "Hủy" }));
    expect(ipc.transferCancel).toHaveBeenCalledWith("lfs-up");
    expect(within(row).getByRole("button", { name: "Đang hủy…" })).toBeDisabled();
  });

  it("explains failures with the bound account and offers retry", async () => {
    const retry = vi.fn();
    const q = useTransferQueue.getState();
    q.start({ opId: "lfs-1", repoId: 1, kind: "lfsUpload", subject: "origin/main" });
    setRetry("lfs-1", retry);
    q.finish("lfs-1", "failed", { failure: failure({ kind: "noPermission" }) });
    q.start({ opId: "lfs-2", repoId: 2, kind: "lfsUpload", subject: "origin/main" });
    q.finish("lfs-2", "failed", {
      failure: failure({
        kind: "quotaExceeded",
        billingUrl: "https://github.com/settings/billing",
      }),
    });
    render(<TransferQueueWidget />);
    const trigger = screen.getByRole("status");
    expect(trigger).toHaveTextContent("Lỗi: 2");

    await userEvent.click(trigger);
    const first = await screen.findByTestId("transfer-lfs-1");
    expect(first).toHaveTextContent("Tài khoản Công ty không có quyền ghi LFS vào repo này.");
    await userEvent.click(within(first).getByRole("button", { name: "Thử lại" }));
    expect(retry).toHaveBeenCalledOnce();

    const second = screen.getByTestId("transfer-lfs-2");
    expect(second).toHaveTextContent("Đã hết dung lượng LFS");
    expect(second).toHaveTextContent("https://github.com/settings/billing");
    expect(within(second).getByRole("button", { name: "Sao chép" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Xóa mục đã xong" }));
    expect(useTransferQueue.getState().order).toEqual([]);
  });

  it("shows a running git transfer with its progress line", () => {
    const q = useTransferQueue.getState();
    q.start({ opId: "push-1", repoId: 1, kind: "push", subject: null });
    q.patch("push-1", { text: "Writing objects 45%", percent: 45 });
    render(<TransferQueueWidget />);
    expect(screen.getByRole("status")).toHaveTextContent("Push");
    expect(screen.getByRole("status")).toHaveTextContent("Writing objects 45%");
  });
});
