import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Dialog } from "@/components/ui/dialog";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { cliSyncKeys } from "@/features/accounts/cli-sync";
import { accountKeys } from "@/features/accounts/queries";
import { initI18n } from "@/i18n";
import { commands, type CliAccountsScan } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { ImportWizard, ImportWizardHost } from "./ImportWizard";
import { onboardingKeys } from "./queries";
import { PROVIDERS, account, makeScan } from "./testData";

const WORK = account("acc_work", "gl-acme", "an.nguyen");

/** What `cli_accounts_scan` reports for the gh account of `makeScan()`. */
const CLI_SCAN: CliAccountsScan = {
  accounts: [
    {
      key: "gh|github.com|an-dev",
      tool: "gh",
      provider: "github",
      host: "github.com",
      apiHost: null,
      username: "an-dev",
      active: true,
      tokenStorage: "keyring",
      existingAccountId: null,
      canUpdate: false,
      providerId: "github",
    },
  ],
  tools: [
    { tool: "gh", installed: true, path: "C:/gh.exe", configFile: "C:/hosts.yml" },
    { tool: "glab", installed: false, path: null, configFile: null },
  ],
  warnings: [],
  newCount: 1,
  shouldNotify: true,
};

function renderWizard(onClose = vi.fn()) {
  render(
    <QueryClientProvider client={queryClient}>
      <Dialog open>
        <ImportWizard onClose={onClose} />
      </Dialog>
    </QueryClientProvider>,
  );
  return onClose;
}

describe("ImportWizard", () => {
  beforeEach(() => {
    initI18n("vi");
    queryClient.clear();
    queryClient.setQueryData(onboardingKeys.scan, makeScan());
    queryClient.setQueryData(accountKeys.providers, PROVIDERS);
    queryClient.setQueryData(cliSyncKeys.scan, CLI_SCAN);
    useAccountDialogs.setState({ manager: null, importWizard: false });
  });
  afterEach(() => vi.restoreAllMocks());

  it("summarises the scan, then lets the user choose and apply", async () => {
    queryClient.setQueryData(accountKeys.list, [WORK]);
    const apply = vi.spyOn(commands, "importApply").mockResolvedValue({
      items: [
        {
          kind: "identity",
          label: "an@acme.vn",
          status: "created",
          code: null,
          message: null,
          id: "idn_1",
        },
        {
          kind: "sshKey",
          label: "C:/Users/an/.ssh/id_acme",
          status: "failed",
          code: "passphraseRequired",
          message: "x",
          id: null,
        },
      ],
      signIn: [{ host: "github.com", username: "an-dev" }],
      importDone: true,
    });
    const user = userEvent.setup();
    renderWizard();

    expect(screen.getByText("Danh tính")).toBeInTheDocument();
    expect(screen.getByText("Tài khoản gh/glab")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tiếp" }));

    // Sections with sources and the alias explanation.
    expect(screen.getByText(/C:\/Users\/an\/\.gitconfig-work:3/)).toBeInTheDocument();
    expect(screen.getAllByText(/không còn cần thiết/).length).toBeGreaterThan(0);
    // gh accounts are added with their token from the CLI panel (PLAN 3B.1).
    expect(screen.getByRole("checkbox", { name: "an-dev @ github.com" })).toBeChecked();

    // The only account is preselected everywhere → 2 identities + 2 rules + 1 key.
    const applyButton = screen.getByRole("button", { name: "Áp dụng (5)" });
    await user.click(screen.getByRole("checkbox", { name: "an@personal.dev" }));
    expect(screen.getByRole("button", { name: "Áp dụng (4)" })).toBe(applyButton);
    await user.click(applyButton);

    expect(apply).toHaveBeenCalledTimes(1);
    const selection = apply.mock.calls[0]![0];
    expect(selection.identities?.map((i) => i.email)).toEqual(["an@acme.vn"]);
    expect(selection.accounts).toEqual([{ host: "github.com", username: "an-dev" }]);
    expect(selection.markDone).toBe(true);

    expect(await screen.findByText(/Đã tạo 1 · đã có 0 · lỗi 1/)).toBeInTheDocument();
    expect(screen.getByText(/nhập passphrase rồi thử lại/)).toBeInTheDocument();
    expect(screen.getByText(/an-dev @ github.com/)).toBeInTheDocument();
  });

  it("without accounts offers sign-in and adds gh accounts from the CLI", async () => {
    queryClient.setQueryData(accountKeys.list, []);
    const importCli = vi.spyOn(commands, "cliAccountsImport").mockResolvedValue([
      {
        key: "gh|github.com|an-dev",
        tool: "gh",
        host: "github.com",
        username: "an-dev",
        status: "added",
        accountId: "acc_dev",
        missingScopes: [],
        fixCommand: null,
        actualUsername: null,
        message: null,
      },
    ]);
    const user = userEvent.setup();
    renderWizard();
    await user.click(screen.getByRole("button", { name: "Tiếp" }));
    expect(screen.getByText(/cần gắn với một tài khoản/)).toBeInTheDocument();
    // Only the key can be imported (unassigned).
    expect(screen.getByRole("button", { name: "Áp dụng (1)" })).toBeEnabled();
    const accounts = screen.getByText("Tài khoản gh / glab").closest("section")!;
    await user.click(within(accounts).getByRole("button", { name: "Thêm 1 tài khoản vào app" }));
    expect(importCli).toHaveBeenCalledWith([
      { tool: "gh", host: "github.com", username: "an-dev" },
    ]);
    expect(await within(accounts).findByText(/Đã thêm — dùng được ngay/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Thêm tài khoản/ }));
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: null,
      add: true,
      reauth: false,
    });
  });

  it("Bỏ qua closes the wizard", async () => {
    queryClient.setQueryData(accountKeys.list, []);
    const user = userEvent.setup();
    const onClose = renderWizard();
    await user.click(screen.getByRole("button", { name: "Bỏ qua" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("the host renders nothing until opened", () => {
    render(
      <QueryClientProvider client={queryClient}>
        <ImportWizardHost />
      </QueryClientProvider>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
