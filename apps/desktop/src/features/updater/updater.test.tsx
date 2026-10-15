import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { AvailableUpdate, UpdateCheck, UpdateProgress, UpdaterStatus } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { checkForUpdates } from "./actions";
import { resetStartupCheck, STARTUP_DELAY_MS } from "./hooks";
import { cleanNotes } from "./notes";
import { updaterKeys } from "./queries";
import { useUpdater } from "./store";
import { UpdaterHost } from "./UpdateDialog";
import { UpdaterSettingsSection } from "./UpdaterSettingsSection";

const ipc = vi.hoisted(() => ({
  updaterStatus: vi.fn(),
  updaterCheck: vi.fn(),
  updaterInstall: vi.fn(),
  updaterSkipVersion: vi.fn(),
  updaterSetChannel: vi.fn(),
}));
const progress = vi.hoisted(() => ({
  emit: null as null | ((e: { payload: UpdateProgress }) => void),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return {
    ...actual,
    commands: { ...actual.commands, ...ipc },
    events: {
      ...actual.events,
      updateProgress: {
        listen: vi.fn(async (cb: (e: { payload: UpdateProgress }) => void) => {
          progress.emit = cb;
          return () => {};
        }),
      },
    },
  };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));
const opener = vi.hoisted(() => ({ openUrl: vi.fn(async () => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => opener);

const RELEASES = "https://github.com/octo/app/releases";

const status = (over: Partial<UpdaterStatus> = {}): UpdaterStatus => ({
  configured: true,
  currentVersion: "0.2.0",
  channel: "stable",
  lastCheck: null,
  skippedVersion: null,
  releasesUrl: RELEASES,
  ...over,
});

const update = (over: Partial<AvailableUpdate> = {}): AvailableUpdate => ({
  version: "0.3.0",
  date: 1_790_856_000,
  notes: "## Có gì mới\n- Sửa lỗi <b>push</b>\n<!-- template comment -->",
  skipped: false,
  releaseUrl: `${RELEASES}/tag/v0.3.0`,
  ...over,
});

const check = (over: Partial<UpdateCheck> = {}): UpdateCheck => ({
  status: "available",
  currentVersion: "0.2.0",
  releasesUrl: RELEASES,
  update: update(),
  ...over,
});

function renderWith(ui: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);
}

beforeAll(() => initI18n("vi"));

beforeEach(() => {
  queryClient.clear();
  for (const f of Object.values(ipc)) f.mockReset();
  ipc.updaterStatus.mockResolvedValue(status());
  useUpdater.getState().close();
  useUi.setState({ notice: null });
  resetStartupCheck();
});

afterEach(() => vi.useRealTimers());

describe("cleanNotes", () => {
  it("keeps text, drops template comments and extra blank lines", () => {
    expect(cleanNotes("a\r\n\r\n\r\n\r\nb <!-- x\ny --> c  \n")).toBe("a\n\nb  c");
    expect(cleanNotes("<!-- unterminated")).toBe("");
    expect(cleanNotes(null)).toBe("");
  });
});

describe("startup check", () => {
  it("waits, then shows the update dialog with notes as plain text", async () => {
    vi.useFakeTimers();
    ipc.updaterCheck.mockResolvedValue(check());
    renderWith(<UpdaterHost />);

    expect(ipc.updaterCheck).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTimeAsync(STARTUP_DELAY_MS));
    expect(ipc.updaterCheck).toHaveBeenCalledWith(true);
    vi.useRealTimers();

    expect(await screen.findByText("Đã có phiên bản mới v0.3.0")).toBeInTheDocument();
    expect(screen.getByText(/Bạn đang dùng v0\.2\.0/)).toBeInTheDocument();
    const notes = screen.getByTestId("update-notes");
    // Raw markup stays text: no <b> element, the comment is gone.
    expect(notes.textContent).toBe("## Có gì mới\n- Sửa lỗi <b>push</b>");
    expect(notes.querySelector("b")).toBeNull();
  });

  it("stays silent for a skipped version, when not configured, or on errors", async () => {
    for (const reply of [
      () => ipc.updaterCheck.mockResolvedValue(check({ update: update({ skipped: true }) })),
      () => ipc.updaterCheck.mockResolvedValue(check({ status: "notConfigured", update: null })),
      () => ipc.updaterCheck.mockResolvedValue(check({ status: "notDue", update: null })),
      () => ipc.updaterCheck.mockRejectedValue({ kind: "offline", message: "dns" }),
    ]) {
      reply();
      vi.spyOn(console, "warn").mockImplementation(() => {});
      await checkForUpdates({ startup: true });
      expect(useUpdater.getState().update).toBeNull();
      expect(useUi.getState().notice).toBeNull();
    }
  });

  it("does not pop up over another dialog", async () => {
    vi.useFakeTimers();
    ipc.updaterCheck.mockResolvedValue(check());
    const blocker = document.createElement("div");
    blocker.setAttribute("role", "dialog");
    document.body.appendChild(blocker);
    renderWith(<UpdaterHost />);

    await act(() => vi.advanceTimersByTimeAsync(STARTUP_DELAY_MS + 10_000));
    expect(ipc.updaterCheck).not.toHaveBeenCalled();
    blocker.remove();
    await act(() => vi.advanceTimersByTimeAsync(5_000));
    expect(ipc.updaterCheck).toHaveBeenCalledTimes(1);
  });
});

describe("update dialog", () => {
  async function openDialog(over: Partial<AvailableUpdate> = {}) {
    ipc.updaterCheck.mockResolvedValue(check({ update: update(over) }));
    renderWith(<UpdaterHost />);
    await act(async () => {
      await checkForUpdates({ startup: false });
    });
    return userEvent.setup();
  }

  it("installs with progress", async () => {
    ipc.updaterInstall.mockReturnValue(new Promise(() => {}));
    const user = await openDialog();
    await user.click(screen.getByRole("button", { name: "Cập nhật & khởi động lại" }));
    expect(ipc.updaterInstall).toHaveBeenCalledWith("0.3.0");

    act(() =>
      progress.emit?.({
        payload: { phase: "downloading", downloaded: 512 * 1024, total: 2048 * 1024, percent: 25 },
      }),
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25");
    expect(screen.getByText("512 KB / 2 MB")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Để sau" })).toBeDisabled();

    act(() =>
      progress.emit?.({
        payload: { phase: "installing", downloaded: 1, total: 1, percent: 100 },
      }),
    );
    expect(screen.getByText("Đang cài đặt…")).toBeInTheDocument();
  });

  it("explains a failed install and offers a retry", async () => {
    ipc.updaterInstall.mockRejectedValue({ kind: "badSignature", message: "minisign: bad" });
    const user = await openDialog();
    await user.click(screen.getByRole("button", { name: "Cập nhật & khởi động lại" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Gói cập nhật không có chữ ký hợp lệ");
    expect(alert).toHaveTextContent("tải bộ cài trên trang Releases");
    expect(screen.getByRole("button", { name: "Thử lại" })).toBeEnabled();
  });

  it("“Để sau” just closes", async () => {
    const user = await openDialog();
    await user.click(screen.getByRole("button", { name: "Để sau" }));
    await waitFor(() => expect(screen.queryByText(/Đã có phiên bản mới/)).toBeNull());
    expect(ipc.updaterSkipVersion).not.toHaveBeenCalled();
    expect(ipc.updaterInstall).not.toHaveBeenCalled();
  });

  it("“Bỏ qua bản này” remembers the version", async () => {
    ipc.updaterSkipVersion.mockResolvedValue(status({ skippedVersion: "0.3.0" }));
    const user = await openDialog();
    await user.click(screen.getByRole("button", { name: "Bỏ qua bản này" }));
    expect(ipc.updaterSkipVersion).toHaveBeenCalledWith("0.3.0");
    await waitFor(() => expect(screen.queryByText(/Đã có phiên bản mới/)).toBeNull());
    expect(queryClient.getQueryData(updaterKeys.status)).toMatchObject({ skippedVersion: "0.3.0" });
  });

  it("a manual check still offers a skipped version", async () => {
    await openDialog({ skipped: true });
    expect(screen.getByText(/Trước đây bạn đã chọn bỏ qua bản này/)).toBeInTheDocument();
  });
});

describe("manual check from the palette", () => {
  it("reports up to date and errors in the notice bar", async () => {
    ipc.updaterCheck.mockResolvedValue(check({ status: "upToDate", update: null }));
    await checkForUpdates({ startup: false });
    expect(ipc.updaterCheck).toHaveBeenCalledWith(false);
    expect(useUi.getState().notice?.message).toBe("Bạn đang dùng phiên bản mới nhất (v0.2.0).");

    ipc.updaterCheck.mockRejectedValue({ kind: "noRelease", message: "404" });
    await checkForUpdates({ startup: false });
    const notice = useUi.getState().notice;
    expect(notice?.kind).toBe("error");
    expect(notice?.message).toBe("Chưa có bản phát hành nào hỗ trợ tự cập nhật");
    expect(notice?.detail).toBe("404");
  });

  it("explains a build without updater and links the Releases page", async () => {
    ipc.updaterCheck.mockResolvedValue(check({ status: "notConfigured", update: null }));
    await checkForUpdates({ startup: false });
    const notice = useUi.getState().notice;
    expect(notice?.message).toBe("Bản build này chưa bật tự cập nhật.");
    notice?.actions?.[0]?.run();
    expect(opener.openUrl).toHaveBeenCalledWith(RELEASES);
  });
});

describe("settings section", () => {
  it("shows that this build has no updater", async () => {
    ipc.updaterStatus.mockResolvedValue(status({ configured: false }));
    renderWith(<UpdaterSettingsSection />);
    expect(await screen.findByText("Bản build này chưa bật tự cập nhật.")).toBeInTheDocument();
    expect(screen.getByText("Phiên bản hiện tại: v0.2.0")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mở trang Releases" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Kiểm tra cập nhật" })).toBeNull();
  });

  it("hides the Releases link when the build has no repository", async () => {
    ipc.updaterStatus.mockResolvedValue(status({ configured: false, releasesUrl: null }));
    renderWith(<UpdaterSettingsSection />);
    expect(await screen.findByText(/không kèm địa chỉ trang phát hành/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Mở trang Releases" })).toBeNull();
  });

  it("checks inline, shows channel, last check and the skipped version", async () => {
    ipc.updaterStatus.mockResolvedValue(
      status({ lastCheck: 1_790_856_000, skippedVersion: "0.3.0" }),
    );
    ipc.updaterCheck.mockResolvedValue(check({ status: "upToDate", update: null }));
    ipc.updaterSkipVersion.mockResolvedValue(status());
    const user = userEvent.setup();
    renderWith(<UpdaterSettingsSection />);

    expect(await screen.findByText("Đang bỏ qua bản v0.3.0.")).toBeInTheDocument();
    expect(screen.getByText(/Lần kiểm tra gần nhất:/)).not.toHaveTextContent("chưa kiểm tra");
    expect(screen.getByRole("combobox", { name: "Kênh cập nhật" })).toHaveTextContent(
      "Ổn định (khuyến nghị)",
    );

    await user.click(screen.getByRole("button", { name: "Kiểm tra cập nhật" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Bạn đang dùng phiên bản mới nhất");
    // Inline only: the notice bar behind the Settings dialog stays empty.
    expect(useUi.getState().notice).toBeNull();

    await user.click(screen.getByRole("button", { name: "Nhắc lại bản này" }));
    expect(ipc.updaterSkipVersion).toHaveBeenCalledWith(null);
  });
});
