import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { initI18n } from "@/i18n";
import {
  commands,
  events,
  type SetupInstallProgress,
  type SetupInstallResult,
  type SetupProgress,
  type SetupStatus,
  type SetupTool,
  type SetupToolsReport,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { pendingSteps, toolLevel, withStepDone } from "./model";
import { setupKeys } from "./queries";
import { SetupHost } from "./SetupWizard";
import { SetupSettingsSection } from "./SetupSettingsSection";
import { useSetupDialog } from "./store";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));

const progress = (p: Partial<SetupProgress> = {}): SetupProgress => ({
  completed: false,
  skipped: false,
  stepsDone: [],
  ...p,
});

const status = (p: Partial<SetupStatus> = {}): SetupStatus => ({
  progress: progress(),
  accountCount: 0,
  importDone: false,
  shouldAutoOpen: true,
  ...p,
});

const tool = (t: Partial<SetupTool> & Pick<SetupTool, "id">): SetupTool => ({
  importance: "required",
  state: "ok",
  version: "2.47.0",
  minimum: null,
  path: "C:/Program Files/Git/cmd/git.exe",
  raw: null,
  downloadUrl: "https://git-scm.com/downloads/win",
  installer: "winget",
  ...t,
});

const report = (gitState: SetupTool["state"]): SetupToolsReport => ({
  tools: [
    tool({
      id: "git",
      state: gitState,
      version: gitState === "missing" ? null : "2.47.0",
      path: gitState === "missing" ? null : "C:/Program Files/Git/cmd/git.exe",
      minimum: "2.35.0",
    }),
    tool({ id: "gitLfs", importance: "recommended", version: "3.5.1", minimum: "3.0.0" }),
    tool({ id: "glab", importance: "optional", state: "missing", version: null, path: null }),
  ],
  canContinue: gitState === "ok",
  restartNeeded: false,
  packageManager: "winget",
  packageManagerFound: true,
});

/** Captures the `setup-install-progress` handler so a test can play installer output. */
function mockProgressEvents() {
  const handlers: ((e: { payload: SetupInstallProgress }) => void)[] = [];
  const unlisten = vi.fn();
  vi.spyOn(events.setupInstallProgress, "listen").mockImplementation(async (h) => {
    handlers.push(h as (e: { payload: SetupInstallProgress }) => void);
    return unlisten;
  });
  const send = (p: SetupInstallProgress) => act(() => handlers.forEach((h) => h({ payload: p })));
  return { send, unlisten };
}

function mockBackend(tools: SetupToolsReport) {
  mockProgressEvents();
  vi.spyOn(commands, "setupTools").mockResolvedValue(tools);
  vi.spyOn(commands, "accountList").mockResolvedValue([]);
  vi.spyOn(commands, "providerList").mockResolvedValue([]);
  vi.spyOn(commands, "appInfo").mockResolvedValue({
    name: "NANOMultipleGitGUI",
    version: "0.1.0",
    os: "windows",
    arch: "x86_64",
    repositoryUrl: null,
  });
  return vi
    .spyOn(commands, "setupSave")
    .mockImplementation(async (p) => status({ progress: p, shouldAutoOpen: false }));
}

function renderHost() {
  render(
    <QueryClientProvider client={queryClient}>
      <SetupHost />
    </QueryClientProvider>,
  );
}

describe("first-run setup", () => {
  beforeEach(() => {
    initI18n("vi");
    queryClient.clear();
    useSetupDialog.setState({ open: false, step: "welcome" });
    useAccountDialogs.setState({ manager: null, clone: null, importWizard: false });
  });
  afterEach(() => vi.restoreAllMocks());

  it("opens by itself for a brand-new user", async () => {
    mockBackend(report("ok"));
    queryClient.setQueryData(setupKeys.status, status());
    renderHost();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Chào mừng đến với NANOMultipleGitGUI")).toBeInTheDocument();
    expect(within(dialog).getByText(/NANO Pret/)).toBeInTheDocument();
    // No repository configured in this build → no link.
    expect(within(dialog).queryByText("Mã nguồn trên GitHub")).toBeNull();
  });

  it("stays closed for an existing user", () => {
    mockBackend(report("ok"));
    queryClient.setQueryData(
      setupKeys.status,
      status({ shouldAutoOpen: false, importDone: true, accountCount: 2 }),
    );
    renderHost();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("does not let the user past the tools step without git", async () => {
    mockBackend(report("missing"));
    queryClient.setQueryData(setupKeys.status, status({ shouldAutoOpen: false }));
    useSetupDialog.setState({ open: true, step: "tools" });
    renderHost();
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(/Chưa có Git/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /Tiếp/ })).toBeDisabled();
    const git = within(dialog).getByRole("listitem", { name: "Git" });
    expect(within(git).getByRole("button", { name: /Cài giúp tôi/ })).toBeEnabled();
    expect(
      within(git).getByRole("button", { name: /Mở trang tải chính thức/ }),
    ).toBeInTheDocument();
  });

  it("lets the user continue once git is there; git-lfs only warns", async () => {
    const ok = report("ok");
    const tools = {
      ...ok,
      tools: ok.tools.map((x) =>
        x.id === "gitLfs" ? { ...x, state: "missing" as const, version: null, path: null } : x,
      ),
    };
    mockBackend(tools);
    queryClient.setQueryData(setupKeys.status, status({ shouldAutoOpen: false }));
    useSetupDialog.setState({ open: true, step: "tools" });
    renderHost();
    const dialog = await screen.findByRole("dialog");
    const lfs = await within(dialog).findByRole("listitem", { name: "Git LFS" });
    expect(within(lfs).getByText(/App vẫn chạy/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /Tiếp/ })).toBeEnabled();
  });

  it("installs a tool through the whitelist command and explains a refused UAC prompt", async () => {
    mockBackend(report("missing"));
    const install = vi.spyOn(commands, "setupInstall").mockResolvedValue({
      tool: "git",
      outcome: "userCancelled",
      manager: "winget",
      command: "winget install --id Git.Git -e",
      exitCode: -1978335226,
      output: "Installer failed with exit code: 1223",
    });
    queryClient.setQueryData(setupKeys.status, status({ shouldAutoOpen: false }));
    useSetupDialog.setState({ open: true, step: "tools" });
    renderHost();
    const user = userEvent.setup();
    const git = await screen.findByRole("listitem", { name: "Git" });
    await user.click(within(git).getByRole("button", { name: /Cài giúp tôi/ }));
    expect(install).toHaveBeenCalledWith("git");
    expect(await within(git).findByText(/Cài đặt bị hủy/)).toBeInTheDocument();
    expect(within(git).getByText("Chi tiết kỹ thuật")).toBeInTheDocument();
  });

  it("shows live install progress and can cancel half way", async () => {
    mockBackend(report("missing"));
    const { send, unlisten } = mockProgressEvents();
    let finish: (r: SetupInstallResult) => void = () => {};
    vi.spyOn(commands, "setupInstall").mockImplementation(
      () => new Promise((resolve) => (finish = resolve)),
    );
    const cancel = vi.spyOn(commands, "setupInstallCancel").mockResolvedValue(true);
    queryClient.setQueryData(setupKeys.status, status({ shouldAutoOpen: false }));
    useSetupDialog.setState({ open: true, step: "tools" });
    renderHost();
    const user = userEvent.setup();
    const git = await screen.findByRole("listitem", { name: "Git" });
    await user.click(within(git).getByRole("button", { name: /Cài giúp tôi/ }));
    expect(await within(git).findByText("Đang chuẩn bị…")).toBeInTheDocument();

    send({ tool: "git", phase: "downloading", percent: 42, recent: ["Downloading https://x"] });
    expect(within(git).getByText("Đang tải xuống… 42%")).toBeInTheDocument();
    expect(within(git).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");
    expect(within(git).getByText("Downloading https://x")).toBeInTheDocument();
    // Another tool's progress is not shown on this row.
    send({ tool: "gh", phase: "installing", percent: null, recent: [] });
    expect(within(git).getByText("Đang tải xuống… 42%")).toBeInTheDocument();

    await user.click(within(git).getByRole("button", { name: "Hủy cài đặt" }));
    expect(cancel).toHaveBeenCalled();
    expect(within(git).getByText("Đang dừng cài đặt…")).toBeInTheDocument();

    await act(async () =>
      finish({
        tool: "git",
        outcome: "cancelled",
        manager: "winget",
        command: "winget install --id Git.Git -e",
        exitCode: null,
        output: "",
      }),
    );
    expect(await within(git).findByText("Đã dừng cài đặt.")).toBeInTheDocument();
    expect(within(git).queryByRole("progressbar")).toBeNull();
    expect(unlisten).toHaveBeenCalled();
  });

  it("skipping remembers the skip and the steps already done", async () => {
    const save = mockBackend(report("ok"));
    queryClient.setQueryData(setupKeys.status, status());
    renderHost();
    const user = userEvent.setup();
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Tiếp/ }));
    expect(useSetupDialog.getState().step).toBe("tools");
    await user.click(within(dialog).getByRole("button", { name: "Bỏ qua hướng dẫn" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(save).toHaveBeenLastCalledWith({
      completed: false,
      skipped: true,
      stepsDone: ["welcome"],
    });
  });

  it("reaching the end marks the setup completed", async () => {
    const save = mockBackend(report("ok"));
    queryClient.setQueryData(setupKeys.status, status({ shouldAutoOpen: false }));
    useSetupDialog.setState({ open: true, step: "accounts" });
    renderHost();
    const user = userEvent.setup();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Chưa có tài khoản nào/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /Tiếp/ }));
    expect(save).toHaveBeenLastCalledWith({
      completed: true,
      skipped: false,
      stepsDone: ["accounts", "finish"],
    });
    await user.click(within(dialog).getByRole("button", { name: /Clone repo/ }));
    expect(useSetupDialog.getState().open).toBe(false);
    expect(useAccountDialogs.getState().clone).toEqual({ accountId: null, url: null });
  });

  it("hides while the sign-in wizard it opened is on screen, then comes back", async () => {
    mockBackend(report("ok"));
    queryClient.setQueryData(setupKeys.status, status({ shouldAutoOpen: false }));
    useSetupDialog.setState({ open: true, step: "accounts" });
    renderHost();
    const user = userEvent.setup();
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Đăng nhập tài khoản mới/ }));
    expect(useAccountDialogs.getState().manager).toMatchObject({ add: true });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    act(() => useAccountDialogs.getState().close("manager"));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(useSetupDialog.getState().step).toBe("accounts");
  });

  it("Settings shows what is left and runs the guide again", async () => {
    queryClient.setQueryData(
      setupKeys.status,
      status({
        shouldAutoOpen: false,
        progress: progress({ skipped: true, stepsDone: ["welcome"] }),
      }),
    );
    useUi.setState({ settingsOpen: true });
    render(
      <QueryClientProvider client={queryClient}>
        <SetupSettingsSection />
      </QueryClientProvider>,
    );
    expect(screen.getByText("Bạn chưa làm xong: Kiểm tra công cụ, Tài khoản.")).toBeInTheDocument();
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Chạy lại hướng dẫn ban đầu" }));
    expect(useSetupDialog.getState()).toMatchObject({ open: true, step: "tools" });
    expect(useUi.getState().settingsOpen).toBe(false);
  });
});

describe("setup model", () => {
  it("tracks steps and levels", () => {
    const p = withStepDone(withStepDone(progress(), "welcome"), "welcome");
    expect(p.stepsDone).toEqual(["welcome"]);
    expect(pendingSteps(p)).toEqual(["tools", "accounts"]);
    expect(pendingSteps({ ...p, completed: true })).toEqual([]);
    expect(toolLevel(tool({ id: "git", state: "missing" }))).toBe("error");
    expect(toolLevel(tool({ id: "gitLfs", importance: "recommended", state: "outdated" }))).toBe(
      "warn",
    );
    expect(toolLevel(tool({ id: "gh", importance: "optional", state: "missing" }))).toBe("info");
    expect(toolLevel(tool({ id: "git", state: "outsidePath" }))).toBe("warn");
  });
});
