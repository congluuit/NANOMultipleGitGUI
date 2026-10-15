import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { StorageOverview, StorageStatus, StorageTargetCheck } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { progressPercent } from "./model";
import { StorageHost } from "./StorageHost";
import { StorageSection } from "./StorageSection";
import { useStorageUi } from "./store";

const GB = 1024 ** 3;
const LOG_PATH = "C:\\Users\\me\\AppData\\Local\\io.nanomgg\\logs";
const CACHE_DEFAULT = "C:\\Users\\me\\AppData\\Local\\io.nanomgg\\cache";

const OVERVIEW: StorageOverview = {
  busy: false,
  locations: [
    {
      kind: "log",
      path: "C:\\Users\\me\\AppData\\Local\\io.nanomgg\\logs",
      defaultPath: "C:\\Users\\me\\AppData\\Local\\io.nanomgg\\logs",
      isDefault: true,
      usedBytes: 2 * 1024 * 1024,
      fileCount: 3,
      usageComplete: true,
      freeBytes: 50 * GB,
      pendingPath: null,
    },
    {
      kind: "cache",
      path: "D:\\nanomgg-cache",
      defaultPath: "C:\\Users\\me\\AppData\\Local\\io.nanomgg\\cache",
      isDefault: false,
      usedBytes: 0,
      fileCount: 0,
      usageComplete: true,
      freeBytes: 200 * GB,
      pendingPath: null,
    },
    {
      kind: "data",
      path: "C:\\Users\\me\\AppData\\Roaming\\io.nanomgg",
      defaultPath: "C:\\Users\\me\\AppData\\Roaming\\io.nanomgg",
      isDefault: true,
      usedBytes: 512 * 1024,
      fileCount: 5,
      usageComplete: false,
      freeBytes: 50 * GB,
      pendingPath: null,
    },
  ],
};

function status(over: Partial<StorageStatus> = {}): StorageStatus {
  return {
    dataDir: "C:\\Users\\me\\AppData\\Roaming\\io.nanomgg",
    dataFreeBytes: 50 * GB,
    lowSpace: false,
    warnings: [],
    pendingData: null,
    ...over,
  };
}

function check(over: Partial<StorageTargetCheck> = {}): StorageTargetCheck {
  return {
    target: "E:\\NanoData",
    createsSubfolder: false,
    isDefault: false,
    neededBytes: 512 * 1024,
    freeBytes: 100 * GB,
    problem: null,
    ...over,
  };
}

const ipc = vi.hoisted(() => ({
  storageOverview: vi.fn(),
  storageStatus: vi.fn(),
  storageCheckTarget: vi.fn(),
  storageChange: vi.fn(),
  storageCancelPending: vi.fn(),
  storageOpen: vi.fn(async () => null),
  storageClearCache: vi.fn(),
  storagePruneLogs: vi.fn(),
  storageRestart: vi.fn(async () => {}),
}));
const listen = vi.hoisted(() => vi.fn(async () => () => {}));
const pickFolder = vi.hoisted(() => vi.fn());
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return {
    ...actual,
    commands: { ...actual.commands, ...ipc },
    events: { ...actual.events, storageMoveProgress: { listen } },
  };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: pickFolder }));

function renderHost(withSection = false) {
  return render(
    <QueryClientProvider client={queryClient}>
      {withSection && <StorageSection />}
      <StorageHost />
    </QueryClientProvider>,
  );
}

async function openDialog() {
  useStorageUi.getState().setOpen(true);
  return screen.findByRole("dialog", { name: "Lưu trữ" });
}

beforeAll(() => initI18n("vi"));

beforeEach(() => {
  queryClient.clear();
  vi.clearAllMocks();
  useUi.setState({ notice: null, settingsOpen: false });
  useStorageUi.setState({ open: false, progress: null, noticesShown: false });
  ipc.storageOverview.mockResolvedValue(OVERVIEW);
  ipc.storageStatus.mockResolvedValue(status());
});

describe("Storage dialog", () => {
  it("lists every folder with size, path and actions", async () => {
    renderHost();
    await openDialog();
    const log = await screen.findByTestId("storage-log");
    expect(within(log).getByText("2 MB · 3 file")).toBeInTheDocument();
    expect(within(log).getByText(LOG_PATH)).toBeInTheDocument();
    expect(within(log).getByRole("button", { name: /Về mặc định/ })).toBeDisabled();
    expect(within(log).getByRole("button", { name: /Xóa log cũ/ })).toBeEnabled();
    const cache = screen.getByTestId("storage-cache");
    expect(within(cache).getByText("Tùy chỉnh")).toBeInTheDocument();
    expect(within(cache).getByRole("button", { name: /Về mặc định/ })).toBeEnabled();
    // Measuring hit its time limit → lower bound.
    const data = screen.getByTestId("storage-data");
    expect(within(data).getByText("≥ 512 KB · 5 file")).toBeInTheDocument();
    expect(within(data).queryByRole("button", { name: /Xóa/ })).toBeNull();
    // Clone folders are the user's code.
    expect(screen.getByText(/app không bao giờ xóa/)).toBeInTheDocument();

    await userEvent.click(within(data).getByRole("button", { name: /Mở thư mục/ }));
    expect(ipc.storageOpen).toHaveBeenCalledWith("data");
  });

  it("explains what clearing the cache does before doing it", async () => {
    ipc.storageClearCache.mockResolvedValue({ removedFiles: 4, removedBytes: 2048, failed: 0 });
    renderHost();
    await openDialog();
    const cache = await screen.findByTestId("storage-cache");
    await userEvent.click(within(cache).getByRole("button", { name: /Xóa toàn bộ cache/ }));
    const confirm = await screen.findByRole("dialog", { name: "Xóa toàn bộ cache?" });
    expect(within(confirm).getByText(/Không mất dữ liệu nào/)).toBeInTheDocument();
    await userEvent.click(within(confirm).getByRole("button", { name: "Xóa toàn bộ cache" }));
    expect(ipc.storageClearCache).toHaveBeenCalled();
    expect(await screen.findByText("Đã xóa cache (4 file, 2 KB).")).toBeInTheDocument();
  });

  it("keeps today's log when deleting old logs", async () => {
    ipc.storagePruneLogs.mockResolvedValue({ removedFiles: 2, removedBytes: 1024, failed: 0 });
    renderHost();
    await openDialog();
    const log = await screen.findByTestId("storage-log");
    await userEvent.click(within(log).getByRole("button", { name: /Xóa log cũ/ }));
    const confirm = await screen.findByRole("dialog", { name: "Xóa log cũ?" });
    expect(within(confirm).getByText(/đang ghi hôm nay được giữ lại/)).toBeInTheDocument();
    await userEvent.click(within(confirm).getByRole("button", { name: "Xóa log cũ" }));
    expect(ipc.storagePruneLogs).toHaveBeenCalled();
  });

  it("shows why a folder cannot be used and does not move", async () => {
    pickFolder.mockResolvedValue("C:\\Users\\me\\AppData\\Local\\io.nanomgg\\logs\\x");
    ipc.storageCheckTarget.mockResolvedValue(check({ problem: { kind: "insideAppFolder" } }));
    renderHost();
    await openDialog();
    const log = await screen.findByTestId("storage-log");
    await userEvent.click(within(log).getByRole("button", { name: /Đổi/ }));
    const confirm = await screen.findByRole("dialog", { name: "Chuyển Log sang thư mục mới" });
    expect(within(confirm).getByRole("alert")).toHaveTextContent(
      /nằm bên trong một thư mục của app/,
    );
    expect(within(confirm).getByRole("button", { name: "Chuyển ngay" })).toBeDisabled();
    expect(ipc.storageChange).not.toHaveBeenCalled();
  });

  it("schedules the data move and offers a restart", async () => {
    pickFolder.mockResolvedValue("E:\\Busy");
    ipc.storageCheckTarget.mockResolvedValue(
      check({ target: "E:\\Busy\\nanomgg-data", createsSubfolder: true }),
    );
    ipc.storageChange.mockImplementation(async () => {
      ipc.storageStatus.mockResolvedValue(status({ pendingData: "E:\\Busy\\nanomgg-data" }));
      return {
        target: "E:\\Busy\\nanomgg-data",
        restartRequired: true,
        oldFilesLeft: 0,
        problem: null,
      };
    });
    renderHost();
    await openDialog();
    const data = await screen.findByTestId("storage-data");
    await userEvent.click(within(data).getByRole("button", { name: /Đổi/ }));
    const confirm = await screen.findByRole("dialog", { name: /Chuyển Dữ liệu app/ });
    expect(within(confirm).getByText(/tạo một thư mục con riêng/)).toBeInTheDocument();
    expect(within(confirm).getByText(/lần khởi động sau/)).toBeInTheDocument();
    await userEvent.click(
      within(confirm).getByRole("button", { name: "Chuyển khi khởi động lại" }),
    );
    expect(ipc.storageChange).toHaveBeenCalledWith("data", "E:\\Busy");

    expect(await screen.findByText("Cần khởi động lại để chuyển dữ liệu app")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Khởi động lại ngay/ }));
    const ask = await screen.findByRole("dialog", { name: "Khởi động lại app?" });
    expect(ipc.storageRestart).not.toHaveBeenCalled();
    await userEvent.click(within(ask).getByRole("button", { name: "Khởi động lại ngay" }));
    expect(ipc.storageRestart).toHaveBeenCalled();
  });

  it("goes back to the default with the default path", async () => {
    ipc.storageCheckTarget.mockResolvedValue(check({ target: CACHE_DEFAULT, isDefault: true }));
    ipc.storageChange.mockResolvedValue({
      target: CACHE_DEFAULT,
      restartRequired: false,
      oldFilesLeft: 0,
      problem: null,
    });
    renderHost();
    await openDialog();
    const cache = await screen.findByTestId("storage-cache");
    await userEvent.click(within(cache).getByRole("button", { name: /Về mặc định/ }));
    expect(ipc.storageCheckTarget).toHaveBeenCalledWith("cache", null);
    const confirm = await screen.findByRole("dialog", { name: /Đưa Cache về thư mục mặc định/ });
    await userEvent.click(within(confirm).getByRole("button", { name: "Chuyển ngay" }));
    expect(ipc.storageChange).toHaveBeenCalledWith("cache", null);
    expect(await screen.findByText(/Đã chuyển Cache sang/)).toBeInTheDocument();
  });

  it("reports a failed move as harmless", async () => {
    pickFolder.mockResolvedValue("F:\\x");
    ipc.storageCheckTarget.mockResolvedValue(check({ target: "F:\\x" }));
    ipc.storageChange.mockRejectedValue({ kind: "internal", message: "F:\\x\\a: access denied" });
    renderHost();
    await openDialog();
    const log = await screen.findByTestId("storage-log");
    await userEvent.click(within(log).getByRole("button", { name: /Đổi/ }));
    const confirm = await screen.findByRole("dialog", { name: /Chuyển Log/ });
    await userEvent.click(within(confirm).getByRole("button", { name: "Chuyển ngay" }));
    expect(await screen.findByText(/Dữ liệu cũ vẫn ở chỗ cũ/)).toBeInTheDocument();
  });
});

describe("Storage notices", () => {
  it("warns when the data drive has less than 1 GB and opens Storage", async () => {
    ipc.storageStatus.mockResolvedValue(
      status({ lowSpace: true, dataFreeBytes: 300 * 1024 * 1024 }),
    );
    renderHost(true);
    await waitFor(() => expect(useUi.getState().notice).not.toBeNull());
    const notice = useUi.getState().notice!;
    expect(notice.message).toBe("Ổ chứa dữ liệu app chỉ còn 300 MB trống");
    expect(screen.getByTestId("storage-settings")).toHaveTextContent("300 MB");
    notice.actions?.[0]?.run();
    expect(await screen.findByRole("dialog", { name: "Lưu trữ" })).toBeInTheDocument();
  });

  it("shows startup warnings once", async () => {
    ipc.storageStatus.mockResolvedValue(
      status({
        warnings: [
          { kind: "dataMoved", from: "C:\\old", to: "E:\\new" },
          {
            kind: "missingFolder",
            storage: "log",
            path: "G:\\logs",
            fallback: "C:\\logs",
          },
        ],
      }),
    );
    renderHost();
    await waitFor(() => expect(useUi.getState().notice).not.toBeNull());
    const notice = useUi.getState().notice!;
    expect(notice.kind).toBe("error");
    expect(notice.message).toBe("Không tìm thấy thư mục Log: G:\\logs");
    expect(notice.hint).toContain("C:\\logs");
    expect(notice.hint).toContain("Đã chuyển dữ liệu app sang E:\\new.");
    useUi.getState().showNotice(null);
    await queryClient.refetchQueries();
    expect(useUi.getState().notice).toBeNull();
  });

  it("opens Storage from the Settings section", async () => {
    useUi.setState({ settingsOpen: true });
    renderHost(true);
    await userEvent.click(await screen.findByRole("button", { name: /Quản lý lưu trữ/ }));
    expect(useUi.getState().settingsOpen).toBe(false);
    expect(useStorageUi.getState().open).toBe(true);
  });
});

describe("progressPercent", () => {
  it("splits copy and verify halves", () => {
    const base = { kind: "log" as const, totalBytes: 100, totalFiles: 2, doneFiles: 1 };
    expect(progressPercent({ ...base, phase: "copying", doneBytes: 50 })).toBe(25);
    expect(progressPercent({ ...base, phase: "verifying", doneBytes: 100 })).toBe(100);
    expect(progressPercent({ ...base, phase: "cleaning", doneBytes: 0 })).toBe(100);
    expect(
      progressPercent({ ...base, phase: "copying", doneBytes: 0, totalBytes: 0, doneFiles: 0 }),
    ).toBe(0);
  });
});
