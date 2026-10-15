import type { ReactNode } from "react";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { CliAccountEntry, CliAccountsScan, CliImportItem, SetupStatus } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useAccountDialogs } from "../dialogs";
import { CliAccountsPanel } from "./CliAccountsPanel";
import { CliSyncNoticeHost } from "./CliSyncNotice";
import { authMethodText } from "./labels";
import { defaultSelection, fixCommandOf, newAccounts } from "./model";

const ipc = vi.hoisted(() => ({
  cliAccountsScan: vi.fn(),
  cliAccountsImport: vi.fn(),
  cliAccountsDismiss: vi.fn(async () => null),
  setupStatus: vi.fn(),
  accountList: vi.fn(async () => []),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));

function entry(
  tool: "gh" | "glab",
  host: string,
  username: string,
  extra: Partial<CliAccountEntry> = {},
): CliAccountEntry {
  return {
    key: `${tool}|${host}|${username}`.toLowerCase(),
    tool,
    provider: tool === "gh" ? "github" : "gitlabSelfHosted",
    host,
    apiHost: null,
    username,
    active: false,
    tokenStorage: "keyring",
    existingAccountId: null,
    canUpdate: false,
    providerId: tool === "gh" ? "github" : null,
    ...extra,
  };
}

const ALICE = entry("gh", "github.com", "alice", { active: true });
const BOB = entry("gh", "github.com", "bob");
const CAROL = entry("gh", "github.com", "carol");
const DAVE = entry("gh", "ghe.acme.vn", "dave");
const GL = entry("glab", "gitlab.acme.vn", "an");
const FRANK = entry("gh", "github.com", "frank", { existingAccountId: "acc_frank" });

function makeScan(accounts: CliAccountEntry[], glabInstalled = false): CliAccountsScan {
  const fresh = accounts.filter((a) => a.existingAccountId === null).length;
  return {
    accounts,
    tools: [
      { tool: "gh", installed: true, path: "C:/gh.exe", configFile: "C:/hosts.yml" },
      { tool: "glab", installed: glabInstalled, path: null, configFile: "C:/glab/config.yml" },
    ],
    warnings: [],
    newCount: fresh,
    shouldNotify: fresh > 0,
  };
}

function result(e: CliAccountEntry, patch: Partial<CliImportItem>): CliImportItem {
  return {
    key: e.key,
    tool: e.tool,
    host: e.host,
    username: e.username,
    status: "added",
    accountId: null,
    missingScopes: [],
    fixCommand: null,
    actualUsername: null,
    message: null,
    ...patch,
  };
}

function wrap(ui: ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);
}

beforeAll(() => initI18n("vi"));

beforeEach(() => {
  queryClient.clear();
  for (const f of Object.values(ipc)) f.mockReset();
  ipc.cliAccountsDismiss.mockResolvedValue(null);
  ipc.accountList.mockResolvedValue([]);
  useAccountDialogs.setState({ manager: null, importWizard: false });
  useUi.setState({ notice: null });
});

describe("cli-sync model", () => {
  it("selects new accounts whose CLI is installed and builds fix commands", () => {
    const scan = makeScan([ALICE, GL, FRANK]);
    expect(newAccounts(scan).map((a) => a.username)).toEqual(["alice", "an"]);
    expect(defaultSelection(scan)).toEqual([ALICE.key]);
    expect(fixCommandOf(result(GL, { status: "noToken" }))).toBe(
      "glab auth login --hostname gitlab.acme.vn",
    );
    expect(fixCommandOf(result(ALICE, { status: "invalidToken" }))).toBe(
      "gh auth login -h github.com",
    );
    expect(fixCommandOf(result(ALICE, { status: "wrongUser" }))).toBe("gh auth status");
    expect(fixCommandOf(result(ALICE, { status: "added" }))).toBeNull();
  });

  it("labels imported accounts by CLI", () => {
    const t = ((k: string, o?: { tool?: string }) => `${k}:${o?.tool ?? ""}`) as never;
    expect(authMethodText(t, { authMethod: "importedCli" }, "gitlabSelfHosted")).toBe(
      "cliSync.source:glab",
    );
    expect(authMethodText(t, { authMethod: "importedCli" }, "github")).toBe("cliSync.source:gh");
    expect(authMethodText(t, { authMethod: "pat" }, "github")).toBe("accounts.authMethod.pat:");
  });
});

describe("CliAccountsPanel", () => {
  it("adds the ticked accounts and explains each result", async () => {
    ipc.cliAccountsScan.mockResolvedValue(makeScan([ALICE, BOB, CAROL, DAVE, GL, FRANK]));
    ipc.cliAccountsImport.mockResolvedValue([
      result(ALICE, { status: "added", accountId: "acc_alice" }),
      result(BOB, {
        status: "missingScopes",
        accountId: "acc_bob",
        missingScopes: ["user:email", "workflow"],
        fixCommand:
          "gh auth switch -h github.com -u bob && gh auth refresh -h github.com -s user:email,workflow",
      }),
      result(DAVE, { status: "wrongUser", actualUsername: "mallory" }),
    ]);
    const onAdded = vi.fn();
    const user = userEvent.setup();
    wrap(<CliAccountsPanel onAdded={onAdded} />);

    expect(
      await screen.findByText(/Tìm thấy 5 tài khoản đã đăng nhập bằng CLI/),
    ).toBeInTheDocument();
    // glab is not installed: its account cannot be ticked and says why.
    const glab = screen.getByRole("checkbox", { name: "an @ gitlab.acme.vn" });
    expect(glab).toBeDisabled();
    expect(screen.getByText(/Không tìm thấy chương trình glab/)).toBeInTheDocument();
    // Existing accounts are folded away.
    expect(screen.getByText("Đã có trong app (1)")).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: "carol @ github.com" }));
    await user.click(screen.getByRole("button", { name: "Thêm 3 tài khoản vào app" }));

    expect(ipc.cliAccountsImport).toHaveBeenCalledWith([
      { tool: "gh", host: "github.com", username: "alice" },
      { tool: "gh", host: "github.com", username: "bob" },
      { tool: "gh", host: "ghe.acme.vn", username: "dave" },
    ]);
    expect(await screen.findByText(/Đã thêm — dùng được ngay/)).toBeInTheDocument();
    expect(onAdded).toHaveBeenCalledTimes(1);

    const bobRow = screen.getByRole("checkbox", { name: "bob @ github.com" }).closest("li")!;
    expect(within(bobRow).getByText(/thiếu quyền: user:email, workflow/)).toBeInTheDocument();
    expect(
      within(bobRow).getByText(/gh auth refresh -h github.com -s user:email,workflow/),
    ).toBeInTheDocument();
    await user.click(within(bobRow).getByRole("button", { name: "Đăng nhập lại trong app" }));
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: "acc_bob",
      add: false,
      reauth: true,
    });

    const daveRow = screen.getByRole("checkbox", { name: "dave @ ghe.acme.vn" }).closest("li")!;
    expect(
      within(daveRow).getByText(/thuộc tài khoản mallory, không phải dave/),
    ).toBeInTheDocument();
    expect(within(daveRow).getByText("gh auth status")).toBeInTheDocument();
    expect(
      within(daveRow).getByRole("button", { name: "Đăng nhập trong app" }),
    ).toBeInTheDocument();
    // Added accounts can no longer be ticked; dave stays ticked for another try, carol unticked.
    expect(screen.getByRole("checkbox", { name: "alice @ github.com" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Thêm 1 tài khoản vào app" })).toBeEnabled();
  });

  it("gets the token again for a missing-scope account and rescans on demand", async () => {
    ipc.cliAccountsScan.mockResolvedValue(makeScan([BOB]));
    ipc.cliAccountsImport.mockResolvedValueOnce([
      result(BOB, {
        status: "missingScopes",
        accountId: "acc_bob",
        missingScopes: ["workflow"],
        fixCommand: "gh auth refresh -h github.com -s workflow",
      }),
    ]);
    const user = userEvent.setup();
    wrap(<CliAccountsPanel compact />);
    await user.click(await screen.findByRole("button", { name: "Thêm 1 tài khoản vào app" }));
    ipc.cliAccountsImport.mockResolvedValueOnce([
      result(BOB, { status: "added", accountId: "acc_bob" }),
    ]);
    await user.click(await screen.findByRole("button", { name: "Lấy lại token từ gh" }));
    expect(ipc.cliAccountsImport).toHaveBeenLastCalledWith([
      { tool: "gh", host: "github.com", username: "bob" },
    ]);
    expect(await screen.findByText(/Đã thêm — dùng được ngay/)).toBeInTheDocument();

    const scans = ipc.cliAccountsScan.mock.calls.length;
    ipc.cliAccountsScan.mockResolvedValue(makeScan([]));
    await user.click(screen.getByRole("button", { name: "Quét lại" }));
    await waitFor(() => expect(ipc.cliAccountsScan.mock.calls.length).toBeGreaterThan(scans));
    expect(
      await screen.findByText(/Không thấy tài khoản nào đăng nhập bằng gh/),
    ).toBeInTheDocument();
  });

  it("reports a CLI that gave no token with the login command", async () => {
    ipc.cliAccountsScan.mockResolvedValue(makeScan([ALICE]));
    ipc.cliAccountsImport.mockResolvedValue([
      result(ALICE, { status: "noToken", message: "no oauth token found" }),
    ]);
    const user = userEvent.setup();
    wrap(<CliAccountsPanel />);
    await user.click(await screen.findByRole("button", { name: "Thêm 1 tài khoản vào app" }));
    expect(await screen.findByText(/gh không đưa token/)).toBeInTheDocument();
    expect(screen.getByText("gh auth login -h github.com")).toBeInTheDocument();
    // Not added: still tickable for another try.
    expect(screen.getByRole("checkbox", { name: "alice @ github.com" })).toBeEnabled();
  });
});

describe("CliSyncNoticeHost", () => {
  const status = (firstRun: boolean): SetupStatus => ({
    progress: { completed: !firstRun, skipped: false, stepsDone: [] },
    importDone: !firstRun,
    accountCount: firstRun ? 0 : 1,
    shouldAutoOpen: firstRun,
  });

  it("shows a light notice for new CLI accounts and remembers Skip", async () => {
    ipc.setupStatus.mockResolvedValue(status(false));
    ipc.cliAccountsScan.mockResolvedValue(makeScan([ALICE, BOB, FRANK]));
    wrap(<CliSyncNoticeHost />);
    await waitFor(() => expect(useUi.getState().notice).not.toBeNull());
    const notice = useUi.getState().notice!;
    expect(notice.message).toMatch(/Tìm thấy 2 tài khoản/);
    expect(notice.actions?.map((a) => a.label)).toEqual(["Mở Quản lý tài khoản", "Bỏ qua"]);
    act(() => notice.actions![1]!.run());
    expect(ipc.cliAccountsDismiss).toHaveBeenCalledWith([ALICE.key, BOB.key]);
    expect(useUi.getState().notice).toBeNull();
  });

  it("opens the account manager from the notice", async () => {
    ipc.setupStatus.mockResolvedValue(status(false));
    ipc.cliAccountsScan.mockResolvedValue(makeScan([ALICE]));
    wrap(<CliSyncNoticeHost />);
    await waitFor(() => expect(useUi.getState().notice).not.toBeNull());
    act(() => useUi.getState().notice!.actions![0]!.run());
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: null,
      add: false,
      reauth: false,
    });
  });

  it("stays silent on first run and when everything was dismissed", async () => {
    ipc.setupStatus.mockResolvedValue(status(true));
    ipc.cliAccountsScan.mockResolvedValue(makeScan([ALICE]));
    const { unmount } = wrap(<CliSyncNoticeHost />);
    await waitFor(() => expect(ipc.setupStatus).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 20));
    expect(ipc.cliAccountsScan).not.toHaveBeenCalled();
    expect(useUi.getState().notice).toBeNull();
    unmount();

    queryClient.clear();
    ipc.setupStatus.mockResolvedValue(status(false));
    ipc.cliAccountsScan.mockResolvedValue({ ...makeScan([ALICE]), shouldNotify: false });
    wrap(<CliSyncNoticeHost />);
    await waitFor(() => expect(ipc.cliAccountsScan).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 20));
    expect(useUi.getState().notice).toBeNull();
  });
});
